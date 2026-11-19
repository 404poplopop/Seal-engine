use crate::catalog;
use crate::command::{
    always_enabled, palette_hits, CommandArgs, CommandRegistry, CommandSpec, PaletteHit,
    PaletteMode, MENU_CATEGORIES,
};
use crate::document::{JournalStep, TextDocument};
use crate::error::CoreError;
use crate::foundation::{Foundation, Modal, ModalKind};
use crate::journal::{self, append_step};
use crate::keymap::{Chord, Keymap};
use crate::layout::Layout;
use crate::messages::Messages;
use crate::models::{self, redo_model, undo_model, EditorModels, Past};
use crate::project::{self, OpenProject};
use seal_world::World;

/// One document offered on the recovery panel after a crash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveredDocument {
    pub id: String,
    pub label_key: String,
    pub path: String,
}

/// Headless editor state. The window renders this and sends commands back into it.
/// Work the window must do, because this crate does not open dialogs or folders.
#[derive(Clone, Debug)]
pub enum HostAction {
    PickFolder {
        create: bool,
    },
    OpenPath(std::path::PathBuf),
    SaveLog,
    SaveElsewhere,
    SaveAs,
    ImportFile,
    /// Ask for a file to copy into the project with `asset.import`.
    ImportAsset,
    ExportFile,
    Restart {
        safe_mode: bool,
    },
    RunExternal {
        exe: String,
        args: Vec<String>,
    },
    /// Ask where to write a shortcut profile. `name` is the profile to export.
    SaveProfile {
        name: String,
    },
    /// Ask which shortcut profile file to read. `name` is used when it is not empty.
    OpenProfile {
        name: String,
    },
    /// Folder picker for copying assets into another project.
    PickMigrateTarget,
}

/// The level parked while one prefab is open for editing.
#[derive(Debug, Clone)]
pub struct PrefabEdit {
    /// Prefab file being edited.
    pub path: std::path::PathBuf,
    /// Name shown in the viewport banner.
    pub name: String,
    /// Root object of the prefab in the editing world.
    pub root: String,
    /// The level as it was, restored on Done.
    pub scene: String,
    pub past: Vec<crate::models::Past>,
    pub future: Vec<crate::models::Past>,
    pub selection: Option<String>,
}

pub struct EditorSession {
    pub foundation: Foundation,
    pub pending_chord: Option<Chord>,
    pub command_hits: Vec<(String, u32)>,
    pub commands: CommandRegistry,
    pub keymap: Keymap,
    pub messages: Messages,
    pub layout: Layout,
    pub layout_epoch: u64,
    pub palette_open: bool,
    pub palette_query: String,
    pub palette_index: usize,
    /// Universal-search index. Rebuilt when a provider's items change.
    pub search_index: std::cell::RefCell<crate::pickers::SearchIndex>,
    pub recent_commands: Vec<String>,
    pub drafts: Vec<(String, String)>,
    pub memory_path: Option<std::path::PathBuf>,
    pub request_close: bool,
    /// The close dialog asked to retry a failed save before exiting.
    pub retry_save_now: bool,
    pub request_save: bool,
    pub save_failed: bool,
    pub save_on_close: bool,
    pub discard_on_close: bool,
    pub project_lock: Option<crate::persist::ProjectGuard>,
    pub scene_dirty: bool,
    /// The scene document tab under the menu bar. Home brings it back.
    pub scene_tab_open: bool,
    /// The scene tab lives in its own native window until it is dragged back.
    pub scene_tab_detached: bool,
    pub settings_dirty: bool,
    pub user_scene: Option<std::path::PathBuf>,
    pub scene_override: Option<std::path::PathBuf>,
    /// A prefab opened on its own. Edits save straight to its file; the level waits here.
    pub prefab_edit: Option<PrefabEdit>,
    pub scene_journal: Option<std::path::PathBuf>,
    pub undo_journal: Option<std::path::PathBuf>,
    /// Documents replayed after an unclean exit. The recovery panel lists these.
    pub recovered: Vec<RecoveredDocument>,
    /// Panel id the window should bring forward on the next frame.
    pub reveal_panel: Option<String>,
    pub layouts_dir: Option<std::path::PathBuf>,
    pub scene_edits: u64,
    journal_silent: bool,
    pub host_action: Option<HostAction>,
    /// Sound preview the host should start or stop (Asset Editor).
    pub audio_request: Option<crate::asset_editor::AudioRequest>,
    /// The sound preview that is playing.
    pub audio_playing: Option<crate::asset_editor::AudioPlaying>,
    /// Project audio changed, or play started: the host applies bus gains before the next mix.
    pub audio_apply: bool,
    /// Text of the open Asset Editor file at the moment it was opened. Edits after that are unsaved.
    pub(crate) asset_saved_text: Option<(String, String)>,
    pub notice: Option<String>,
    pub keymap_path: Option<std::path::PathBuf>,
    pub documents: Vec<TextDocument>,
    pub journal_path: Option<std::path::PathBuf>,
    pub project: Option<OpenProject>,
    pub wizard_open: bool,
    pub recent_path: Option<std::path::PathBuf>,
    pub models: EditorModels,
    pub outliner_scroll: usize,
    pub world: World,
    /// Smoothed time between frames, for the stats readout.
    pub frame_ms: f32,
    /// A packaged game is running: the viewport shows the scene with no editor marks.
    pub game_view: bool,
    /// Milliseconds since this session started. Toasts and notices use it as a clock.
    pub uptime_ms: u64,
    pub past: Vec<Past>,
    pub future: Vec<Past>,
    /// Scene text at project open or the last verified save; Undo History row zero.
    pub undo_baseline: String,
    /// Command whose title names the next undo step pushed during dispatch.
    pending_undo_command: Option<String>,
    /// The logic graph open in the Graph panel. Every edit is saved to its file.
    pub graph: Option<crate::graph_doc::GraphDoc>,
    /// The sequence open in the Timeline. Every edit is saved to its file.
    pub timeline: Option<crate::timeline_doc::TimelineDoc>,
    /// The last-open graph was looked up for this project already.
    pub graph_restore_tried: bool,
    /// The assistant link while it is switched on.
    pub agent_link: Option<crate::agent_link::AgentLink>,
    /// Why the link could not start (shown in the Assistant panel).
    pub agent_link_error: Option<String>,
    /// Assistant commands waiting for the user to allow them.
    pub agent_pending: Vec<crate::agent_link::PendingCall>,
    /// What assistants did, oldest first; also kept in the project's log file.
    pub agent_log: Vec<crate::agent_link::AgentEntry>,
    pub agent_serial: u64,
    pub remote_control: Option<crate::editor_remote::RemoteControl>,
    pub remote_error: Option<String>,
    pub remote_ws_queue: Vec<String>,
    pub remote_screenshot: Option<Vec<u8>>,
    /// Set by a remote screenshot request; the window captures the next frame for it.
    pub remote_screenshot_wanted: bool,
    /// Selection last sent to remote WebSocket clients, so a change is pushed once.
    pub remote_sent_selection: Option<(Vec<String>, Option<String>)>,
}

/// Drafts that hold a tool mode rather than typed input; they are not restored.
const TRANSIENT_DRAFTS: &[&str] = &["place-brush", "tile-erase"];

impl EditorSession {
    pub fn new(layout: Layout) -> Self {
        let foundation = Foundation::new();
        let messages = crate::editor_language::messages_for_store(&foundation.settings);
        let mut session = Self {
            foundation,
            pending_chord: None,
            command_hits: Vec::new(),
            commands: CommandRegistry::default(),
            keymap: Keymap::default(),
            messages,
            layout,
            layout_epoch: 0,
            palette_open: false,
            palette_query: String::new(),
            palette_index: 0,
            search_index: std::cell::RefCell::new(crate::pickers::SearchIndex::default()),
            recent_commands: Vec::new(),
            drafts: Vec::new(),
            memory_path: None,
            request_close: false,
            retry_save_now: false,
            request_save: false,
            save_failed: false,
            save_on_close: false,
            discard_on_close: false,
            project_lock: None,
            scene_dirty: false,
            scene_tab_open: true,
            scene_tab_detached: false,
            settings_dirty: false,
            user_scene: None,
            scene_override: None,
            prefab_edit: None,
            scene_journal: None,
            undo_journal: None,
            recovered: Vec::new(),
            reveal_panel: None,
            layouts_dir: None,
            scene_edits: 0,
            journal_silent: false,
            host_action: None,
            audio_request: None,
            audio_playing: None,
            audio_apply: false,
            asset_saved_text: None,
            notice: None,
            keymap_path: None,
            documents: vec![TextDocument::new("scratch")],
            journal_path: None,
            project: None,
            wizard_open: false,
            recent_path: None,
            models: EditorModels::default(),
            outliner_scroll: 0,
            world: World::demo(),
            frame_ms: 0.0,
            game_view: false,
            uptime_ms: 0,
            past: Vec::new(),
            future: Vec::new(),
            undo_baseline: String::new(),
            pending_undo_command: None,
            graph: None,
            timeline: None,
            graph_restore_tried: false,
            agent_link: None,
            agent_link_error: None,
            agent_pending: Vec::new(),
            agent_log: Vec::new(),
            agent_serial: 0,
            remote_control: None,
            remote_error: None,
            remote_ws_queue: Vec::new(),
            remote_screenshot: None,
            remote_screenshot_wanted: false,
            remote_sent_selection: None,
        };
        match Keymap::defaults() {
            Ok(keymap) => session.keymap = keymap,
            Err(err) => session.notice = Some(err.to_string()),
        }
        let registered = session.register_commands();
        note_startup(&mut session, registered);
        let panels = models::register_model_commands(&mut session);
        note_startup(&mut session, panels);
        let foundation = catalog::register_foundation(&mut session);
        note_startup(&mut session, foundation);
        session.sync_undo_baseline();
        let module = session.install_module(&crate::module::CorePanels);
        note_startup(&mut session, module);
        session.adopt_world();
        apply_default_workspace(&mut session);
        session
    }

    /// Copy the scene into the outliner rows.
    /// True for a 2D scene: it draws pictures (sprites) and has no 3D shapes.
    pub fn scene_is_2d(&self) -> bool {
        let entities = &self.world.entities;
        entities.iter().any(|e| !e.sprite.is_empty())
            && entities.iter().all(|e| e.shape.is_none() || !e.sprite.is_empty())
    }

    /// A 2D scene always opens straight-on in the flat Front view, never in the
    /// angled perspective view, whatever view was saved last time.
    pub fn flatten_view_for_2d_scene(&mut self) {
        if self.scene_is_2d() && self.models.viewport_view == "perspective" {
            self.models.viewport_view = "front".into();
            let _ = self.set_draft("viewport-yaw", "0");
            let _ = self.set_draft("viewport-pitch", "0");
        }
        if self.scene_is_2d() {
            for pane in &mut self.models.viewport_panes {
                if pane.view == "perspective" {
                    pane.view = crate::viewport_split::FLAT_2D_VIEW.into();
                    pane.yaw = 0.0;
                    pane.pitch = 0.0;
                }
            }
        }
    }

    pub fn adopt_world(&mut self) {
        let mut notices: Vec<(String, String, String)> = Vec::new();
        let mut repair_ids: Vec<String> = Vec::new();
        self.models.nodes = self
            .world
            .entities
            .iter()
            .map(|entity| {
                let mut changed = false;
                let fields = entity
                    .inspector_fields()
                    .into_iter()
                    .map(|(key, value)| {
                        let (mut next, err) =
                            crate::inspector_fields::sanitize_enum_field(&key, &value);
                        if let Some(msg) = err {
                            notices.push((
                                entity.id.clone(),
                                msg.to_string(),
                                format!("{key}\t{value}"),
                            ));
                            changed = true;
                        }
                        if crate::inspector_fields::field_spec(&key).is_some_and(|spec| {
                            spec.kind == crate::inspector_fields::InspectorFieldKind::Number
                        }) {
                            let clamped = crate::inspector_fields::clamp_number_field(&key, &next);
                            if clamped != value {
                                changed = true;
                            }
                            next = clamped;
                        } else if next != value {
                            changed = true;
                        }
                        (key, next)
                    })
                    .collect();
                if changed {
                    repair_ids.push(entity.id.clone());
                }
                crate::models::MockNode {
                    id: entity.id.clone(),
                    name: entity.name.clone(),
                    parent: entity.parent.clone(),
                    fields,
                }
            })
            .collect();
        for (_id, msg, payload) in notices {
            let (field, value) = payload.split_once('\t').unwrap_or((payload.as_str(), ""));
            let line = self.messages.format(&msg, &[("field", field), ("value", value)]);
            seal_log::warn!("{line}");
        }
        for id in repair_ids {
            self.mirror_node(&id);
        }
    }

    /// Write one outliner row back onto its entity.
    pub fn mirror_node(&mut self, id: &str) {
        let Some(node) = self.models.nodes.iter().find(|node| node.id == id).cloned() else {
            return;
        };
        let now = self.world.play_time;
        let Some(entity) = self.world.entity_mut(id) else {
            return;
        };
        entity.name = node.name;
        entity.parent = node.parent.clone();
        for (key, value) in &node.fields {
            match key.as_str() {
                "role" => entity.tag = value.clone(),
                "x" => entity.translation[0] = value.parse().unwrap_or(entity.translation[0]),
                "y" => entity.translation[1] = value.parse().unwrap_or(entity.translation[1]),
                "z" => entity.translation[2] = value.parse().unwrap_or(entity.translation[2]),
                "rx" => entity.rotation[0] = value.parse().unwrap_or(entity.rotation[0]),
                "ry" => entity.rotation[1] = value.parse().unwrap_or(entity.rotation[1]),
                "rz" => entity.rotation[2] = value.parse().unwrap_or(entity.rotation[2]),
                "sx" => entity.scale[0] = value.parse().unwrap_or(entity.scale[0]),
                "sy" => entity.scale[1] = value.parse().unwrap_or(entity.scale[1]),
                "sz" => entity.scale[2] = value.parse().unwrap_or(entity.scale[2]),
                "r" => {
                    entity.color[0] =
                        value.parse::<f32>().unwrap_or(entity.color[0]).clamp(0.0, 1.0)
                }
                "g" => {
                    entity.color[1] =
                        value.parse::<f32>().unwrap_or(entity.color[1]).clamp(0.0, 1.0)
                }
                "b" => {
                    entity.color[2] =
                        value.parse::<f32>().unwrap_or(entity.color[2]).clamp(0.0, 1.0)
                }
                "hidden" => entity.hidden = value == "yes",
                "locked" => entity.locked = value == "yes",
                "unselectable" => entity.unselectable = value == "yes",
                "roughness" => entity.roughness = parse_unit(value, entity.roughness),
                "mesh_texture" | "mesh_layer" | "mesh_slice" | "mesh_tile_u" | "mesh_tile_v" => {
                    seal_world::mesh_surface::apply(entity, key, value);
                }
                "metal" => entity.metal = parse_unit(value, entity.metal),
                "intensity" => entity.intensity = value.parse().unwrap_or(entity.intensity),
                "fov" => entity.fov = value.parse().unwrap_or(entity.fov),
                "near" => entity.near = value.parse().unwrap_or(entity.near),
                "far" => entity.far = value.parse().unwrap_or(entity.far),
                "shape" => entity.shape = seal_world::Shape::parse(value),
                "motion" => entity.dynamic = value == "dynamic",
                "friction" => {
                    entity.friction =
                        value.trim().parse::<f32>().map_or(entity.friction, |v| v.clamp(0.0, 10.0))
                }
                "bounce" => {
                    entity.restitution = value
                        .trim()
                        .parse::<f32>()
                        .map_or(entity.restitution, |v| v.clamp(0.0, 1.0))
                }
                "walk_speed" => {
                    entity.walk_speed = value
                        .trim()
                        .parse::<f32>()
                        .map_or(entity.walk_speed, |v| v.clamp(0.0, 50.0))
                }
                "source_kind" => entity.source_kind = value.clone(),
                "source_asset" => entity.source_asset = value.clone(),
                "sprite" => {
                    entity.sprite = value.trim().replace('\\', "/");
                    if entity.pivot == [0.5, 0.5] {
                        if let Some(root) = &self.project {
                            let path = std::path::Path::new(&entity.sprite);
                            let full = if path.is_relative() {
                                root.root.join(path)
                            } else {
                                path.to_path_buf()
                            };
                            entity.pivot = crate::import_presets::picture_pivot(&full);
                        }
                    }
                }
                "normal_map" => entity.normal_map = value.trim().replace('\\', "/"),
                "parallax" => {
                    if let Some(p) = seal_world::parse_parallax(value) {
                        entity.parallax = p;
                    }
                }
                "repeat_x" => entity.repeat_x = value == "yes",
                "light2d" => {
                    let kind = value.trim();
                    if kind.is_empty() || crate::light2d::Kind::parse(kind).is_some() {
                        entity.light2d.kind = kind.to_string();
                        if kind == "freeform" {
                            seal_world::light_fields::seed_square(entity);
                        }
                    }
                }
                "light_blend" | "light_falloff" | "light_points" | "light_angle" => {
                    seal_world::light_fields::apply(entity, key, value);
                }
                "light_range" => {
                    entity.light2d.range = value
                        .trim()
                        .parse::<f32>()
                        .map_or(entity.light2d.range, |v| v.clamp(0.01, 10000.0))
                }
                "light_cone" => {
                    entity.light2d.cone = value
                        .trim()
                        .parse::<f32>()
                        .map_or(entity.light2d.cone, |v| v.clamp(1.0, 360.0))
                }
                "light_shadows" => entity.light2d.shadows = value == "yes",
                "camera2d" => entity.camera2d.on = value == "yes",
                "cam_follow" => entity.camera2d.follow = value.trim().to_string(),
                "cam_smoothing" => {
                    entity.camera2d.smoothing = value
                        .trim()
                        .parse::<f32>()
                        .ok()
                        .filter(|v| v.is_finite())
                        .map_or(entity.camera2d.smoothing, |v| v.clamp(0.0, 10.0))
                }
                "cam_zoom" => {
                    entity.camera2d.zoom = value
                        .trim()
                        .parse::<f32>()
                        .ok()
                        .filter(|v| v.is_finite())
                        .map_or(entity.camera2d.zoom, |v| v.clamp(0.05, 50.0))
                }
                "cam_limits" => {
                    if let Some(l) = seal_world::parse_limits(value) {
                        entity.camera2d.limits = l;
                    }
                }
                "cam_pixels" => {
                    entity.camera2d.pixel_snap = value
                        .trim()
                        .parse::<f32>()
                        .ok()
                        .filter(|v| v.is_finite())
                        .map_or(entity.camera2d.pixel_snap, |v| v.clamp(0.0, 1024.0))
                }
                "cam_cull_mask" => {
                    if let Some(bits) = seal_world::phys2d::parse_layers(value) {
                        entity.camera2d.cull_mask = bits;
                    }
                }
                k if seal_world::cam_drag::apply(entity, k, value) => {}
                "audio_listener" => entity.audio_listener.on = value == "yes",
                "listener_max_distance" => {
                    entity.audio_listener.max_distance = value
                        .trim()
                        .parse::<f32>()
                        .map_or(entity.audio_listener.max_distance, |v| v.clamp(0.1, 10_000.0))
                }
                "listener_rolloff" => {
                    entity.audio_listener.rolloff = value
                        .trim()
                        .parse::<f32>()
                        .map_or(entity.audio_listener.rolloff, |v| v.clamp(0.01, 8.0))
                }
                k if seal_world::sprite_mask::KEYS.contains(&k) => {
                    seal_world::sprite_mask::apply(entity, k, value);
                }
                k if seal_world::material2d::KEYS.contains(&k)
                    || seal_world::material2d::SHEET_KEYS.contains(&k) =>
                {
                    seal_world::material2d::apply(entity, k, value);
                }
                k if seal_world::shape2d::known(k) => {
                    seal_world::shape2d::apply(entity, k, value);
                }
                k if seal_world::sprite_shape::KEYS.contains(&k) => {
                    seal_world::sprite_shape::apply(entity, k, value);
                }
                k if seal_world::nav2d::KEYS.contains(&k) => {
                    seal_world::nav2d::apply(&mut entity.nav2d, k, value);
                }
                k if seal_world::character2d::KEYS.contains(&k) => {
                    seal_world::character2d::apply(&mut entity.character2d, k, value);
                }
                k if seal_world::phys2d::KEYS.contains(&k) => {
                    // Refused text leaves the setting as it was.
                    seal_world::phys2d::apply(&mut entity.phys2d, k, value);
                }
                k if seal_world::area_effect::KEYS.contains(&k) => {
                    seal_world::area_effect::apply(&mut entity.area, &mut entity.surface, k, value);
                }
                k if seal_world::volumes::KEYS.contains(&k) => {
                    seal_world::volumes::apply(&mut entity.volume, k, value);
                }
                k if seal_world::post_volume::KEYS.contains(&k) => {
                    seal_world::post_volume::apply(&mut entity.post, k, value);
                }
                "script_order" => {
                    entity.script_order = value
                        .trim()
                        .parse::<f32>()
                        .map_or(entity.script_order, |v| v.round() as i32)
                }
                "script_group" => seal_world::script_groups::apply(&mut entity.script_group, value),
                k if seal_world::remote_transform::KEYS.contains(&k) => {
                    seal_world::remote_transform::apply(&mut entity.remote, k, value);
                }
                "layer" => {
                    entity.layer =
                        value.trim().parse::<f32>().map_or(entity.layer, |v| v.round() as i32)
                }
                "frames" => {
                    entity.frames = value
                        .trim()
                        .parse::<f32>()
                        .map_or(entity.frames, |v| v.round().clamp(1.0, 256.0) as u32)
                }
                "fps" => {
                    entity.fps =
                        value.trim().parse::<f32>().map_or(entity.fps, |v| v.clamp(0.0, 120.0))
                }
                "animation" => entity.play_animation(value.trim(), now),
                "flip_x" => entity.flip[0] = value == "yes",
                "flip_y" => entity.flip[1] = value == "yes",
                "socket" => entity.socket = value.trim().to_string(),
                "tint" => {
                    if let Some(t) = seal_world::parse_tint(value) {
                        let hex = value.trim().trim_start_matches('#');
                        let hex_len = if hex.len() == 3 { 6 } else { hex.len() };
                        if hex_len == 8 {
                            entity.tint = t;
                        } else {
                            entity.tint = [t[0], t[1], t[2], entity.tint[3]];
                        }
                    }
                }
                "opacity" => {
                    entity.tint[3] =
                        value.trim().parse::<f32>().map_or(entity.tint[3], |v| v.clamp(0.0, 1.0))
                }
                "slice" => {
                    if let Some(s) = seal_world::parse_slice(value) {
                        entity.slice = s;
                    }
                }
                k if seal_world::sprite_slice::KEYS.contains(&k) => {
                    seal_world::sprite_slice::apply(entity, k, value);
                }
                k if seal_world::sprite_pivot::KEYS.contains(&k) => {
                    seal_world::sprite_pivot::apply(entity, k, value);
                }
                k if seal_world::sprite_sort::KEYS.contains(&k) => {
                    seal_world::sprite_sort::apply(entity, k, value);
                }
                "order" => {
                    entity.order =
                        value.trim().parse::<f32>().map_or(entity.order, |v| v.round() as i32)
                }
                "script" => entity.script = value.trim().replace('\\', "/"),
                "particles" => entity.particles = value.trim().replace('\\', "/"),
                "unsaved" => entity.outliner_unsaved = value == "yes",
                "spawned_play" => entity.outliner_play_spawned = value == "yes",
                "outliner_warning" => entity.outliner_warning_key = value.clone(),
                _ => {}
            }
        }
        self.mark_scene();
    }

    fn push_motion(&mut self) {
        for entity in &self.world.entities {
            let Some(node) = self.models.nodes.iter_mut().find(|node| node.id == entity.id) else {
                continue;
            };
            for (axis, key) in [(0, "x"), (1, "y"), (2, "z")] {
                let text = format_axis(entity.translation[axis]);
                if let Some(field) = node.fields.iter_mut().find(|(name, _)| name == key) {
                    field.1 = text;
                }
            }
        }
    }

    /// Advance the play loop. Editing and pause do not move the scene.
    pub fn tick(&mut self, dt: f32) {
        let step_ms = (dt * 1000.0).max(0.0) as u64;
        self.uptime_ms = self.uptime_ms.saturating_add(step_ms);
        crate::content::poll_content_watcher(self);
        crate::aseprite::poll_open(self);
        if let Err(err) = crate::import_pipeline::pump_steps(self, 4) {
            let text = err.to_string();
            self.models.long_job = None;
            self.notify(text, true);
        }
        crate::layered_picture::poll_open(self);
        // Smoothed so the readout can be read.
        self.frame_ms += (dt * 1000.0 - self.frame_ms) * 0.1;
        crate::agent_link::poll(self);
        crate::editor_remote::poll(self);
        self.sync_graph_debug();
        if self.world.step(dt) {
            // A scene switch, a removed pickup or a spawned object changes the object set;
            // then the Outliner, Inspector and viewport rows are rebuilt, not just moved.
            let same_objects = self.models.nodes.len() == self.world.entities.len()
                && self
                    .models
                    .nodes
                    .iter()
                    .zip(&self.world.entities)
                    .all(|(node, entity)| node.id == entity.id && node.name == entity.name);
            if same_objects {
                self.push_motion();
            } else {
                self.adopt_world();
            }
        }
        if let Some(doc) = self.graph.as_mut() {
            doc.debug = self.world.debug_view(&doc.label);
        }
        self.forward_play_log();
    }

    /// Give play the open graph's breakpoints when they changed.
    pub fn sync_graph_debug(&mut self) {
        let Some(doc) = self.graph.as_ref() else { return };
        let have = self.world.breakpoints.get(&doc.label);
        if have.map_or(doc.breakpoints.is_empty(), |b| *b == doc.breakpoints) {
            return;
        }
        let (label, nodes) = (doc.label.clone(), doc.breakpoints.clone());
        self.world.set_breakpoints(&label, nodes);
    }

    /// Continue play stopped at a breakpoint, all the way or by one node.
    pub fn debug_continue(&mut self, single: bool) {
        self.sync_graph_debug();
        if single {
            self.world.debug_step();
        } else {
            self.world.play();
        }
        self.push_motion();
        if let Some(doc) = self.graph.as_mut() {
            doc.debug = self.world.debug_view(&doc.label);
        }
        self.forward_play_log();
    }

    /// Send what object scripts printed during play to the Console.
    pub fn forward_play_log(&mut self) {
        for line in std::mem::take(&mut self.world.play_log) {
            if let Some(text) = line.strip_prefix("warning: ") {
                seal_log::log(seal_log::Level::Warn, "sequence", text.to_string());
            } else {
                seal_log::log(seal_log::Level::Info, "script", line);
            }
        }
    }

    pub fn dock_scene_tab(&mut self) {
        self.scene_tab_detached = false;
        self.scene_tab_open = true;
    }

    /// Lines shown in the toolbar Save tooltip when something is dirty.
    pub fn save_tooltip_detail(&self, layout_dirty: bool) -> String {
        let mut lines = Vec::new();
        if self.scene_dirty {
            let name = self.scene_file_name();
            lines.push(self.messages.format("toolbar-save-line-scene", &[("name", name.as_str())]));
        }
        if self.settings_dirty {
            lines.push(self.messages.get("toolbar-save-line-settings"));
        }
        if self.documents.iter().any(|doc| doc.id == "scratch" && doc.dirty()) {
            lines.push(self.messages.get("toolbar-save-line-scratch"));
        }
        if layout_dirty {
            lines.push(self.messages.get("toolbar-save-line-layout"));
        }
        if lines.is_empty() {
            return self.messages.get("tooltip-file-save-clean");
        }
        let header = self.messages.get("tooltip-file-save-dirty");
        format!("{header}\n{}", lines.join("\n"))
    }

    pub fn toolbar_save_enabled(&self, layout_dirty: bool) -> bool {
        self.dirty_count() > 0 || layout_dirty
    }

    /// Title and shortcut of a command, for toolbar tooltips and menus.
    pub fn command_label(&self, id: &str) -> (String, String) {
        let title = match id {
            "file.quit" | "app.quit" => self.messages.get("menu-quit"),
            "view.reset-layout" => self.messages.get("menu-reset-layout"),
            "layout.save" => self.messages.get("menu-save-layout"),
            _ => self
                .commands
                .get(id)
                .map(|spec| self.messages.get(&spec.title_key))
                .unwrap_or_else(|| id.to_string()),
        };
        let shortcut = self.keymap.shortcut(id);
        (title, shortcut)
    }

    /// The open graph, when the Graph panel has focus: Undo, Redo and Delete then act on it.
    pub fn graph_in_focus(&mut self) -> Option<&mut crate::graph_doc::GraphDoc> {
        if self.foundation.focused_panel_id == "graph" {
            self.graph.as_mut()
        } else {
            None
        }
    }

    pub fn mark_scene(&mut self) {
        if self.world.mode != seal_world::PlayMode::Editing {
            return;
        }
        if let Some(edit) = &self.prefab_edit {
            // Prefab edits are saved to the prefab file right away, so nothing is ever lost.
            let path = edit.path.clone();
            if let Some(text) = self.world.prefab_text(&edit.root) {
                if let Err(err) = crate::persist::atomic_write(&path, text.as_bytes()) {
                    self.notice = Some(err.to_string());
                }
            }
            return;
        }
        self.scene_dirty = true;
        self.scene_edits = self.scene_edits.saturating_add(1);
        if self.journal_silent {
            return;
        }
        if let Err(err) = self.write_journals() {
            self.notice = Some(err.to_string());
        }
    }

    /// Label for the command that is running and may push undo.
    pub(crate) fn undo_step_label_now(&self) -> String {
        let Some(id) = &self.pending_undo_command else {
            return String::new();
        };
        let title_key = self.commands.get(id).map(|spec| spec.title_key.as_str()).unwrap_or(id);
        self.messages.get(title_key)
    }

    pub fn undo_step_limit(&self) -> usize {
        self.foundation
            .settings
            .get("undo.max_steps")
            .and_then(|text| text.parse::<usize>().ok())
            .unwrap_or(200)
            .max(1)
    }

    /// Row index in the Undo History panel (0 is the opened or saved scene).
    pub fn undo_history_position(&self) -> usize {
        self.past.len()
    }

    /// Jump the scene to a history row. Index 0 is the saved or opened scene.
    pub fn jump_undo_history(&mut self, index: usize) -> Result<(), CoreError> {
        self.journal_silent = true;
        let result = (|| {
            while self.past.len() > index {
                undo_active(self, &CommandArgs::default())?;
            }
            while self.past.len() < index {
                redo_active(self, &CommandArgs::default())?;
            }
            if index == 0 {
                let baseline = self.undo_baseline.clone();
                crate::models::restore_scene(self, &baseline);
            }
            Ok(())
        })();
        self.journal_silent = false;
        if result.is_ok() {
            self.mark_scene();
        }
        result
    }

    pub fn sync_undo_baseline(&mut self) {
        self.undo_baseline = self.world.to_text();
    }

    pub fn push_past(&mut self, item: crate::models::Past) {
        self.past.push(item);
        let steps = self
            .foundation
            .settings
            .get("undo.max_steps")
            .and_then(|text| text.parse::<usize>().ok())
            .unwrap_or(200)
            .max(1);
        let extra = self.past.len().saturating_sub(steps);
        if extra > 0 {
            self.past.drain(0..extra);
        }
        let budget = self
            .foundation
            .settings
            .get("undo.max_memory_mb")
            .and_then(|text| text.parse::<usize>().ok())
            .unwrap_or(64)
            .saturating_mul(1024 * 1024);
        while self.past.len() > 1
            && self.past.iter().map(crate::models::past_weight).sum::<usize>() > budget
        {
            self.past.remove(0);
        }
    }

    fn write_journals(&mut self) -> Result<(), CoreError> {
        let current = self.world.to_text().into_bytes();
        let history = crate::models::encode_history(&self.past);
        if let Some(path) = self.scene_journal.clone() {
            crate::journal_crc::write_records(
                &path,
                &[crate::journal_crc::JournalRecord {
                    kind: 1,
                    seq: self.scene_edits,
                    payload: current,
                }],
            )?;
        }
        if let Some(path) = self.undo_journal.clone() {
            crate::journal_crc::write_records(
                &path,
                &[crate::journal_crc::JournalRecord {
                    kind: 4,
                    seq: self.scene_edits,
                    payload: history,
                }],
            )?;
        }
        Ok(())
    }

    /// Apply a scene journal left by a crash, then restore the undo stack.
    pub fn recover_journals(&mut self) -> Result<bool, CoreError> {
        let mut recovered = false;
        if let Some(path) = self.scene_journal.clone() {
            let records = crate::journal_crc::read_records(&path)?;
            if let Some(record) = records.into_iter().rev().find(|record| record.kind == 1) {
                if !record.payload.is_empty() {
                    let text = String::from_utf8(record.payload)
                        .map_err(|err| CoreError::new(err.to_string()))?;
                    let loaded = seal_world::World::from_text(&text).map_err(CoreError::new);
                    self.journal_silent = false;
                    self.world = loaded?;
                    self.adopt_world();
                    self.scene_dirty = true;
                    recovered = true;
                }
            }
        }
        if let Some(path) = self.undo_journal.clone() {
            let records = crate::journal_crc::read_records(&path)?;
            if let Some(record) = records.into_iter().rev().find(|record| record.kind == 4) {
                self.past = crate::models::decode_history(&record.payload);
            }
        }
        Ok(recovered)
    }

    /// Open the recovery panel after an unclean exit or a replayed scene journal.
    /// The list names the scene when it was replayed, and the scratch journal when it has steps.
    pub fn present_recovery(
        &mut self,
        scene_recovered: bool,
        unclean: bool,
    ) -> Result<(), CoreError> {
        if !scene_recovered && !unclean {
            return Ok(());
        }
        let mut recovered = Vec::new();
        if scene_recovered {
            recovered.push(RecoveredDocument {
                id: "scene".into(),
                label_key: "recovery-doc-scene".into(),
                path: file_name_of(self.scene_path().as_deref()),
            });
        }
        if let Some(path) = self.journal_path.clone() {
            let steps = journal::read_steps(&path)?;
            if !steps.is_empty() {
                recovered.push(RecoveredDocument {
                    id: "scratch".into(),
                    label_key: "recovery-doc-scratch".into(),
                    path: file_name_of(Some(path.as_path())),
                });
            }
        }
        self.recovered = recovered;
        if self.recovered.is_empty() {
            return Ok(());
        }
        self.focus_panel("recovery");
        self.notice = Some(self.recovery_summary());
        Ok(())
    }

    /// The scene journal is still waiting for restore, keep both, or discard.
    pub fn awaiting_scene_recovery(&self) -> bool {
        self.recovered.iter().any(|doc| doc.id == "scene")
    }

    /// Heading and status text: how many documents were recovered.
    pub fn recovery_summary(&self) -> String {
        let count = self.recovered.len();
        let count_text = count.to_string();
        let noun_id = if count == 1 { "recovery-noun-one" } else { "recovery-noun-many" };
        let noun = self.messages.get(noun_id);
        self.messages.format(
            "recovery-summary",
            &[("count", count_text.as_str()), ("documents", noun.as_str())],
        )
    }

    pub fn focus_panel(&mut self, id: &str) {
        self.foundation.focused_panel_id = id.to_string();
        if let Some(index) = models::PANEL_ORDER.iter().position(|panel| *panel == id) {
            self.models.focused_panel = index;
        }
        if !self.foundation.open_panels.iter().any(|panel| panel == id) {
            self.foundation.open_panels.push(id.to_string());
        }
        self.reveal_panel = Some(id.to_string());
    }

    /// Write the recovered scene and drop the scene journal. The undo stack stays.
    pub fn restore_recovered_scene(&mut self) -> Result<(), CoreError> {
        self.save_recovered_scene()?;
        self.recovered.retain(|doc| doc.id != "scene");
        Ok(())
    }

    /// Save the recovered scene and keep a copy of the scene journal beside it.
    pub fn keep_recovered_scene(&mut self) -> Result<(), CoreError> {
        self.copy_scene_journal_aside()?;
        self.save_recovered_scene()?;
        self.recovered.retain(|doc| doc.id != "scene");
        Ok(())
    }

    /// Drop the recovered edits and load the scene that was last saved.
    pub fn discard_recovered_scene(&mut self) -> Result<(), CoreError> {
        let path = self.scene_path().ok_or_else(|| CoreError::new("no scene to reload"))?;
        if !path.exists() {
            return Err(CoreError::new("saved scene is missing"));
        }
        self.load_scene(&path)?;
        self.past.clear();
        self.future.clear();
        remove_if_present(self.scene_journal.as_deref())?;
        remove_if_present(self.undo_journal.as_deref())?;
        self.recovered.retain(|doc| doc.id != "scene");
        Ok(())
    }

    fn save_recovered_scene(&mut self) -> Result<(), CoreError> {
        let path = self.scene_path().ok_or_else(|| CoreError::new("no scene to save"))?;
        crate::persist::atomic_write(&path, self.world.to_text().as_bytes())?;
        self.scene_dirty = false;
        if let Some(journal) = self.scene_journal.clone() {
            crate::journal_crc::clear_records(&journal)?;
        }
        Ok(())
    }

    fn copy_scene_journal_aside(&self) -> Result<(), CoreError> {
        let Some(journal) = self.scene_journal.clone() else {
            return Ok(());
        };
        let records = crate::journal_crc::read_records(&journal)?;
        if records.is_empty() {
            return Ok(());
        }
        let kept = kept_journal_path(&journal)?;
        std::fs::copy(&journal, &kept)?;
        Ok(())
    }

    pub fn load_named_layouts(&mut self) -> Result<(), CoreError> {
        let Some(dir) = self.layouts_dir.clone() else {
            return Ok(());
        };
        if !dir.exists() {
            return Ok(());
        }
        for entry in std::fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("seal") {
                continue;
            }
            let Some(name) = path.file_stem().and_then(|stem| stem.to_str()).map(str::to_string)
            else {
                continue;
            };
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(value) = seal_sealtext::parse(&text) else {
                continue;
            };
            if Layout::from_value(&value).is_err() {
                continue;
            }
            self.foundation.layouts.retain(|(id, _)| id != &name);
            self.foundation.layouts.push((name, text));
        }
        Ok(())
    }

    /// File name shown on the scene tab. A scratch session is Untitled.
    pub fn scene_file_name(&self) -> String {
        let raw = file_name_of(self.scene_path().as_deref());
        let scratch = self.project.is_none()
            && self.scene_override.is_none()
            && (raw.is_empty() || raw == "scene.seal");
        if scratch || raw.is_empty() {
            self.messages.get("scene-untitled")
        } else {
            raw
        }
    }

    /// Drop unsaved scene edits and close the scene tab.
    pub fn revert_scene_tab(&mut self) -> bool {
        if let Some(path) = self.scene_path() {
            if path.is_file() {
                if let Err(err) = self.load_scene(&path) {
                    self.notice = Some(err.to_string());
                    return false;
                }
                self.scene_tab_open = false;
                return true;
            }
        }
        if let Some(journal) = self.scene_journal.clone() {
            if let Err(err) = crate::journal_crc::clear_records(&journal) {
                self.notice = Some(err.to_string());
                return false;
            }
        }
        self.world = World::demo();
        self.adopt_world();
        self.past.clear();
        self.future.clear();
        self.scene_dirty = false;
        self.scene_tab_open = false;
        true
    }

    pub fn dirty_marks(&self, save_confirmed: bool) -> crate::dirty::DirtyMarks {
        let scene_name = self.scene_file_name();
        let project = self.project.as_ref().map(|project| project.name.as_str()).unwrap_or("");
        let any =
            self.scene_dirty || self.settings_dirty || self.documents.iter().any(|doc| doc.dirty());
        crate::dirty::dirty_marks(project, &scene_name, any, save_confirmed)
    }

    /// The scratch journal is the stored text after a clean exit, so it is not unsaved work.
    pub fn settle_loaded_scratch(&mut self) {
        if let Some(doc) = self.documents.iter_mut().find(|doc| doc.id == "scratch") {
            doc.mark_saved();
        }
    }

    pub fn panel_dirty(&self, panel: &str) -> bool {
        crate::dirty::panel_is_dirty(
            panel,
            self.scene_dirty,
            self.settings_dirty,
            self.documents.iter().map(|doc| (doc.id.as_str(), doc.dirty())),
        )
    }

    pub fn scene_path(&self) -> Option<std::path::PathBuf> {
        if let Some(path) = &self.scene_override {
            return Some(path.clone());
        }
        if let Some(project) = &self.project {
            return Some(project.root.join("scene.seal"));
        }
        self.user_scene.clone()
    }

    /// Point the next scene write at another file and keep the edit dirty until it lands.
    pub fn save_scene_elsewhere(&mut self, path: std::path::PathBuf) {
        self.scene_override = Some(path);
        self.scene_dirty = true;
    }

    pub fn scene_snapshot(&self) -> Option<(std::path::PathBuf, Vec<u8>)> {
        let path = self.scene_path()?;
        if let Some(edit) = &self.prefab_edit {
            return Some((path, edit.scene.clone().into_bytes()));
        }
        Some((path, self.world.to_text().into_bytes()))
    }

    /// The scene file is already on disk. Clear the dirty flag only if nothing newer arrived.
    pub fn note_scene_saved(&mut self, edits: u64) -> Result<(), CoreError> {
        if self.scene_edits != edits {
            return Ok(());
        }
        self.scene_dirty = false;
        if let Some(journal) = self.scene_journal.clone() {
            crate::journal_crc::clear_records(&journal)?;
        }
        if self.foundation.settings.get("undo.clear_on_save") == Some("true") {
            self.past.clear();
            self.future.clear();
            if let Some(journal) = self.undo_journal.clone() {
                crate::journal_crc::clear_records(&journal)?;
            }
        }
        self.sync_undo_baseline();
        Ok(())
    }

    pub fn draft(&self, key: &str) -> Option<String> {
        self.drafts.iter().find(|(name, _)| name == key).map(|(_, value)| value.clone())
    }

    /// A draft is marked when it is still different from the committed text.
    pub fn draft_is_marked(&self, key: &str, committed: &str) -> bool {
        self.draft(key).is_some_and(|value| value != committed)
    }

    pub fn set_draft(&mut self, key: &str, value: &str) -> Result<(), CoreError> {
        if let Some(slot) = self.drafts.iter_mut().find(|(name, _)| name == key) {
            slot.1 = value.to_string();
        } else {
            self.drafts.push((key.to_string(), value.to_string()));
        }
        self.save_memory()
    }

    pub fn clear_draft(&mut self, key: &str) -> Result<(), CoreError> {
        self.drafts.retain(|(name, _)| name != key);
        self.save_memory()
    }

    pub fn save_memory(&mut self) -> Result<(), CoreError> {
        let Some(path) = self.memory_path.clone() else {
            return Ok(());
        };
        crate::persist::atomic_write(&path, &self.memory_bytes())?;
        Ok(())
    }

    pub fn load_memory(&mut self) -> Result<(), CoreError> {
        let Some(path) = self.memory_path.clone() else {
            return Ok(());
        };
        let Some(bytes) = crate::persist::read_durable(&path)? else {
            return Ok(());
        };
        let text = String::from_utf8(bytes).map_err(|err| CoreError::new(err.to_string()))?;
        self.apply_memory(&text)
    }

    fn memory_bytes(&self) -> Vec<u8> {
        let drafts = seal_sealtext::SealValue::Array(
            self.drafts
                .iter()
                .map(|(key, value)| {
                    seal_sealtext::SealValue::Array(vec![
                        seal_sealtext::SealValue::String(key.clone()),
                        seal_sealtext::SealValue::String(value.clone()),
                    ])
                })
                .collect(),
        );
        let recent = seal_sealtext::SealValue::Array(
            self.recent_commands.iter().cloned().map(seal_sealtext::SealValue::String).collect(),
        );
        let notices = seal_sealtext::SealValue::Array(
            self.foundation
                .notices
                .history()
                .iter()
                .map(|notice| {
                    seal_sealtext::SealValue::Array(vec![
                        seal_sealtext::SealValue::String(notice.text.clone()),
                        seal_sealtext::SealValue::Bool(notice.error),
                        seal_sealtext::SealValue::Number(notice.at_ms.to_string()),
                    ])
                })
                .collect(),
        );
        seal_sealtext::SealValue::Map(
            vec![("drafts".into(), drafts), ("recent".into(), recent), ("notices".into(), notices)]
                .into(),
        )
        .to_text()
        .into_bytes()
    }

    fn apply_memory(&mut self, text: &str) -> Result<(), CoreError> {
        let value = seal_sealtext::parse(text)?;
        let Some(map) = value.as_map() else {
            return Ok(());
        };
        if let Some(seal_sealtext::SealValue::Array(items)) =
            map.iter().find(|(k, _)| k == "drafts").map(|(_, v)| v)
        {
            self.drafts = items
                .iter()
                .filter_map(|item| match item {
                    seal_sealtext::SealValue::Array(pair) => {
                        let key = pair.first().and_then(|v| match v {
                            seal_sealtext::SealValue::String(text) => Some(text.clone()),
                            _ => None,
                        })?;
                        let value = pair.get(1).and_then(|v| match v {
                            seal_sealtext::SealValue::String(text) => Some(text.clone()),
                            _ => None,
                        })?;
                        Some((key, value))
                    }
                    _ => None,
                })
                // A paint brush is a tool you pick up, not typed work: it never
                // comes back armed after a restart.
                .filter(|(key, _)| !TRANSIENT_DRAFTS.contains(&key.as_str()))
                .collect();
        }
        if let Some(seal_sealtext::SealValue::Array(items)) =
            map.iter().find(|(k, _)| k == "recent").map(|(_, v)| v)
        {
            self.recent_commands = items
                .iter()
                .filter_map(|item| match item {
                    seal_sealtext::SealValue::String(text) => Some(text.clone()),
                    _ => None,
                })
                .collect();
        }
        if let Some(seal_sealtext::SealValue::Array(items)) =
            map.iter().find(|(k, _)| k == "notices").map(|(_, v)| v)
        {
            let notices = items
                .iter()
                .enumerate()
                .filter_map(|(index, item)| match item {
                    seal_sealtext::SealValue::Array(parts) => {
                        let text = match parts.first() {
                            Some(seal_sealtext::SealValue::String(text)) => text.clone(),
                            _ => return None,
                        };
                        let error =
                            matches!(parts.get(1), Some(seal_sealtext::SealValue::Bool(true)));
                        let at_ms = match parts.get(2) {
                            Some(seal_sealtext::SealValue::Number(text)) => {
                                text.parse().unwrap_or(0)
                            }
                            _ => 0,
                        };
                        Some(crate::foundation::Notice { id: index as u64 + 1, text, error, at_ms })
                    }
                    _ => None,
                })
                .collect();
            self.foundation.notices.restore(notices);
        }
        Ok(())
    }

    pub fn logging_ceiling(&self) -> seal_log::Level {
        let verbose = self.foundation.settings.get("log.verbose") == Some("true");
        let level = self.foundation.settings.get("log.level").unwrap_or("info");
        seal_log::ceiling_for(level, verbose)
    }

    pub fn logging_notice(&self) -> Option<String> {
        if self.foundation.settings.get("log.verbose") == Some("true") {
            Some(self.messages.get("log-verbose-slow"))
        } else {
            None
        }
    }

    pub fn settings_bytes(&self) -> Vec<u8> {
        self.foundation.settings.user_text().into_bytes()
    }

    pub fn outliner_query_for(&self, panel_id: &str) -> String {
        let base = crate::panel::panel_base(panel_id);
        if base == "outliner" && panel_id == "outliner" {
            return self.models.outliner_query.clone();
        }
        if base == "outliner" {
            return self.models.panel_states.get(panel_id).cloned().unwrap_or_default();
        }
        self.models.outliner_query.clone()
    }

    pub fn set_outliner_query_for(&mut self, panel_id: &str, query: String) {
        let base = crate::panel::panel_base(panel_id);
        if base == "outliner" && panel_id == "outliner" {
            self.models.outliner_query = query;
        } else if base == "outliner" {
            self.models.panel_states.insert(panel_id.to_string(), query);
        } else {
            self.models.outliner_query = query;
        }
    }

    pub fn inspector_query_for(&self, panel_id: &str) -> String {
        let base = crate::panel::panel_base(panel_id);
        if base == "inspector" && panel_id == "inspector" {
            return self.models.inspector_query.clone();
        }
        if base == "inspector" {
            return self
                .models
                .panel_states
                .get(&format!("{panel_id}:query"))
                .cloned()
                .unwrap_or_default();
        }
        self.models.inspector_query.clone()
    }

    pub fn set_inspector_query_for(&mut self, panel_id: &str, query: String) {
        let base = crate::panel::panel_base(panel_id);
        if base == "inspector" && panel_id == "inspector" {
            self.models.inspector_query = query;
        } else if base == "inspector" {
            self.models.panel_states.insert(format!("{panel_id}:query"), query);
        } else {
            self.models.inspector_query = query;
        }
    }

    fn store_loaded_draft(&mut self, key: &str, value: &str) {
        if let Some(slot) = self.drafts.iter_mut().find(|(name, _)| name == key) {
            slot.1 = value.to_string();
        } else {
            self.drafts.push((key.to_string(), value.to_string()));
        }
    }

    pub fn panel_state_bytes(&self) -> Vec<u8> {
        let expanded = seal_sealtext::SealValue::Array(
            self.models.expanded.iter().cloned().map(seal_sealtext::SealValue::String).collect(),
        );
        let viewport_bookmarks = seal_sealtext::SealValue::Map(
            self.models
                .viewport_bookmarks
                .iter()
                .map(|(slot, bookmark)| {
                    (
                        slot.clone(),
                        seal_sealtext::SealValue::Map(
                            vec![
                                (
                                    "view".into(),
                                    seal_sealtext::SealValue::String(bookmark.view.clone()),
                                ),
                                (
                                    "selection".into(),
                                    match &bookmark.selection {
                                        Some(id) => seal_sealtext::SealValue::String(id.clone()),
                                        None => seal_sealtext::SealValue::Null,
                                    },
                                ),
                                (
                                    "selection_ids".into(),
                                    seal_sealtext::SealValue::Array(
                                        bookmark
                                            .selection_ids
                                            .iter()
                                            .cloned()
                                            .map(seal_sealtext::SealValue::String)
                                            .collect(),
                                    ),
                                ),
                                (
                                    "yaw".into(),
                                    seal_sealtext::SealValue::Number(bookmark.yaw.to_string()),
                                ),
                                (
                                    "pitch".into(),
                                    seal_sealtext::SealValue::Number(bookmark.pitch.to_string()),
                                ),
                                (
                                    "distance".into(),
                                    seal_sealtext::SealValue::Number(bookmark.distance.to_string()),
                                ),
                                (
                                    "pan_x".into(),
                                    seal_sealtext::SealValue::Number(bookmark.pan_x.to_string()),
                                ),
                                (
                                    "pan_y".into(),
                                    seal_sealtext::SealValue::Number(bookmark.pan_y.to_string()),
                                ),
                                (
                                    "zoom".into(),
                                    seal_sealtext::SealValue::Number(bookmark.zoom.to_string()),
                                ),
                            ]
                            .into(),
                        ),
                    )
                })
                .collect::<Vec<_>>()
                .into(),
        );
        seal_sealtext::SealValue::Map(
            vec![
                ("schema_version".into(), seal_sealtext::SealValue::Number("1".into())),
                (
                    "view".into(),
                    seal_sealtext::SealValue::String(self.models.viewport_view.clone()),
                ),
                (
                    "tool".into(),
                    seal_sealtext::SealValue::String(self.models.viewport_tool.clone()),
                ),
                ("snap".into(), seal_sealtext::SealValue::Bool(self.models.snap)),
                ("grid".into(), seal_sealtext::SealValue::Bool(self.models.viewport_grid)),
                (
                    "selection".into(),
                    match &self.models.selection {
                        Some(id) => seal_sealtext::SealValue::String(id.clone()),
                        None => seal_sealtext::SealValue::Null,
                    },
                ),
                ("expanded".into(), expanded),
                (
                    "outliner".into(),
                    seal_sealtext::SealValue::String(self.models.outliner_query.clone()),
                ),
                (
                    "scroll".into(),
                    seal_sealtext::SealValue::Number(self.outliner_scroll.to_string()),
                ),
                ("theme".into(), seal_sealtext::SealValue::String(self.models.theme.clone())),
                ("scale".into(), seal_sealtext::SealValue::Number(self.models.scale.to_string())),
                (
                    "content".into(),
                    seal_sealtext::SealValue::String(self.models.content_query.clone()),
                ),
                (
                    "console".into(),
                    seal_sealtext::SealValue::String(self.models.console_query.clone()),
                ),
                (
                    "settings_query".into(),
                    seal_sealtext::SealValue::String(self.models.settings_query.clone()),
                ),
                (
                    "folder".into(),
                    match &self.models.content_root {
                        Some(path) => seal_sealtext::SealValue::String(path.display().to_string()),
                        None => seal_sealtext::SealValue::Null,
                    },
                ),
                ("content_list".into(), seal_sealtext::SealValue::Bool(self.models.content_list)),
                ("viewport_bookmarks".into(), viewport_bookmarks),
                (
                    "viewport_yaw".into(),
                    seal_sealtext::SealValue::String(
                        self.draft("viewport-yaw").unwrap_or_else(|| "0".into()),
                    ),
                ),
                (
                    "viewport_pitch".into(),
                    seal_sealtext::SealValue::String(
                        self.draft("viewport-pitch").unwrap_or_else(|| "0".into()),
                    ),
                ),
                (
                    "viewport_distance".into(),
                    seal_sealtext::SealValue::String(
                        self.draft("viewport-distance").unwrap_or_else(|| "10".into()),
                    ),
                ),
                (
                    "draft_viewport_pan_x".into(),
                    seal_sealtext::SealValue::String(
                        self.draft("viewport-pan-x").unwrap_or_else(|| "0".into()),
                    ),
                ),
                (
                    "draft_viewport_pan_y".into(),
                    seal_sealtext::SealValue::String(
                        self.draft("viewport-pan-y").unwrap_or_else(|| "0".into()),
                    ),
                ),
                (
                    "inspector".into(),
                    seal_sealtext::SealValue::String(self.models.inspector_query.clone()),
                ),
                (
                    "inspector_sections".into(),
                    seal_sealtext::SealValue::Map(
                        self.models
                            .inspector_sections
                            .iter()
                            .map(|(id, open)| (id.clone(), seal_sealtext::SealValue::Bool(*open)))
                            .collect::<Vec<_>>()
                            .into(),
                    ),
                ),
                (
                    "viewport_pan_x".into(),
                    seal_sealtext::SealValue::Number(self.models.viewport_pan_x.to_string()),
                ),
                (
                    "viewport_pan_y".into(),
                    seal_sealtext::SealValue::Number(self.models.viewport_pan_y.to_string()),
                ),
                (
                    "viewport_zoom".into(),
                    seal_sealtext::SealValue::Number(self.models.viewport_zoom.to_string()),
                ),
                (
                    "viewport_split".into(),
                    seal_sealtext::SealValue::String(crate::viewport_split::encode_split(self)),
                ),
            ]
            .into_iter()
            .chain(
                self.models
                    .panel_states
                    .iter()
                    .map(|(id, text)| (id.clone(), seal_sealtext::SealValue::String(text.clone()))),
            )
            .chain(self.models.inspector_locks.iter().map(|(panel, entity)| {
                (
                    format!("inspector_lock:{panel}"),
                    seal_sealtext::SealValue::String(entity.clone()),
                )
            }))
            .collect::<Vec<_>>()
            .into(),
        )
        .to_text()
        .into_bytes()
    }

    pub fn save_scene(&mut self) -> Result<(), CoreError> {
        let Some(path) = self.scene_path() else {
            return Ok(());
        };
        let text = match &self.prefab_edit {
            Some(edit) => edit.scene.clone(),
            None => self.world.to_text(),
        };
        if self.prefab_edit.is_none() && self.world.objects_per_file {
            seal_world::object_files::write(&path, &text).map_err(CoreError::new)?;
        } else {
            crate::persist::atomic_write(&path, text.as_bytes())?;
            if self.prefab_edit.is_none() {
                let _ = seal_world::object_files::clear_folder(&path);
            }
        }
        if let Some(project) = self.project.as_ref() {
            if let Ok(rel) = path.strip_prefix(&project.root) {
                let rel_s = rel.to_string_lossy().replace('\\', "/");
                let _ =
                    crate::asset_thumbnails::refresh_scene_thumbnail(&project.root, &rel_s, &text);
            }
        }
        self.scene_dirty = false;
        if let Some(journal) = self.scene_journal.clone() {
            crate::journal_crc::clear_records(&journal)?;
        }
        if self.foundation.settings.get("undo.clear_on_save") == Some("true") {
            self.past.clear();
            self.future.clear();
            if let Some(journal) = self.undo_journal.clone() {
                crate::journal_crc::clear_records(&journal)?;
            }
        }
        self.sync_undo_baseline();
        Ok(())
    }

    /// Save the open scene, then make `path` the scene being edited. The choice is
    /// remembered in `.seal-editor/open-scene.txt` so the project reopens on it.
    pub fn open_scene_file(&mut self, path: std::path::PathBuf) -> Result<(), CoreError> {
        let text = std::fs::read_to_string(&path)
            .map_err(|_| CoreError::new(format!("scene {} was not found", path.display())))?;
        let text = seal_world::object_files::expand(&path, &text).map_err(CoreError::new)?;
        let world = World::from_text(&text).map_err(CoreError::new)?;
        self.finish_prefab_edit()?;
        if self.scene_dirty {
            self.save_scene()?;
        }
        let root = self.project.as_ref().map(|p| p.root.clone());
        let main = root.as_ref().map(|r| r.join("scene.seal"));
        self.scene_override =
            if main.as_deref() == Some(path.as_path()) { None } else { Some(path.clone()) };
        if let Some(root) = &root {
            let note = root.join(".seal-editor").join("open-scene.txt");
            let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
            if let Some(dir) = note.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = crate::persist::atomic_write(&note, rel.as_bytes());
        }
        self.world = world;
        self.refresh_prefabs();
        self.adopt_world();
        self.flatten_view_for_2d_scene();
        self.models.selection = None;
        self.foundation.selection.clear();
        self.past.clear();
        self.future.clear();
        self.sync_undo_baseline();
        if let Some(journal) = self.undo_journal.clone() {
            crate::journal_crc::clear_records(&journal)?;
        }
        if let Some(journal) = self.scene_journal.clone() {
            crate::journal_crc::clear_records(&journal)?;
        }
        self.scene_dirty = false;
        self.scene_tab_open = true;
        let name = file_name_of(Some(path.as_path()));
        self.notify(self.messages.format("scene-opened", &[("name", name.as_str())]), false);
        Ok(())
    }

    /// Open a prefab file on its own. The level is parked until `finish_prefab_edit`.
    pub fn begin_prefab_edit(&mut self, path: std::path::PathBuf) -> Result<(), CoreError> {
        if self.prefab_edit.is_some() {
            self.finish_prefab_edit()?;
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|_| CoreError::new(format!("prefab {} was not found", path.display())))?;
        let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let mut world = seal_world::World::from_text(&seal_world::World::empty_scene_text(&name))
            .map_err(CoreError::new)?;
        if let Some(root) = self.project.as_ref() {
            world.project_dir = Some(root.root.clone());
        }
        let root = world.add_prefab_text(&text, None).map_err(CoreError::new)?;
        let scene = self.world.to_text();
        self.world = world;
        self.prefab_edit = Some(PrefabEdit {
            path,
            name,
            root: root.clone(),
            scene,
            past: std::mem::take(&mut self.past),
            future: std::mem::take(&mut self.future),
            selection: self.models.selection.clone(),
        });
        self.adopt_world();
        self.models.selection = Some(root);
        Ok(())
    }

    /// Save the open prefab, bring the level back and rebuild its copies (one undo step).
    pub fn finish_prefab_edit(&mut self) -> Result<(), CoreError> {
        let Some(edit) = self.prefab_edit.take() else {
            return Ok(());
        };
        if let Some(text) = self.world.prefab_text(&edit.root) {
            let mut saved = text;
            if let Some(project) = self.project.as_ref() {
                let prefabs = project.root.join("prefabs");
                if let Ok(raw) = std::fs::read_to_string(&edit.path) {
                    if let Ok(value) = seal_sealtext::parse(&raw) {
                        if let Some(seal_sealtext::SealValue::String(base)) = value.get("base") {
                            saved = seal_world::prefab_variant::compact_variant_file(
                                &prefabs, base, &saved, &edit.name,
                            )
                            .map_err(CoreError::new)?;
                        }
                    }
                }
            }
            crate::persist::atomic_write(&edit.path, saved.as_bytes())?;
        }
        self.world = seal_world::World::from_text(&edit.scene).map_err(CoreError::new)?;
        self.past = edit.past;
        self.future = edit.future;
        self.refresh_prefabs();
        self.adopt_world();
        self.models.selection = edit.selection.filter(|id| self.world.entity(id).is_some());
        let next = self.world.to_text();
        if next != edit.scene {
            crate::models::push_scene_past(self, edit.scene);
            self.mark_scene();
        }
        Ok(())
    }

    /// Rebuild every prefab copy in the scene from `prefabs/*.prefab` in the project.
    /// Copies keep their placement and the fields changed on them. Returns the count.
    pub fn refresh_prefabs(&mut self) -> usize {
        let Some(dir) = self.project.as_ref().map(|p| p.root.join("prefabs")) else {
            return 0;
        };
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return 0;
        };
        let mut files: Vec<std::path::PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "prefab"))
            .collect();
        files.sort();
        let mut count = 0;
        for file in files {
            let Some(stem) = file.file_stem().map(|s| s.to_string_lossy().to_string()) else {
                continue;
            };
            if !self
                .world
                .entities
                .iter()
                .any(|e| e.source_kind == "prefab" && e.source_asset == stem)
            {
                continue;
            }
            match std::fs::read_to_string(&file).map_err(|e| e.to_string()).and_then(|text| {
                let expanded =
                    seal_world::prefab_variant::read_prefab_for_world(Some(&dir), &text, false)?;
                self.world.refresh_prefab_instances(&stem, &expanded)
            }) {
                Ok(n) => count += n,
                Err(err) => self.notify(format!("Prefab {stem} could not be read: {err}"), true),
            }
        }
        count += seal_world::prefab_nest::refresh_nested(&mut self.world, &dir);
        count
    }

    pub fn load_scene(&mut self, path: &std::path::Path) -> Result<(), CoreError> {
        let Some(bytes) = crate::persist::read_durable(path)? else {
            return Ok(());
        };
        let text = String::from_utf8(bytes).map_err(|err| CoreError::new(err.to_string()))?;
        let text = seal_world::object_files::expand(path, &text).map_err(CoreError::new)?;
        self.world = seal_world::World::from_text(&text).map_err(CoreError::new)?;
        self.prefab_edit = None;
        self.refresh_prefabs();
        self.adopt_world();
        self.scene_dirty = false;
        self.past.clear();
        self.future.clear();
        self.sync_undo_baseline();
        Ok(())
    }

    pub fn save_settings(&mut self, path: &std::path::Path) -> Result<(), CoreError> {
        crate::persist::atomic_write(path, self.foundation.settings.user_text().as_bytes())?;
        self.settings_dirty = false;
        Ok(())
    }

    pub fn load_settings(&mut self, path: &std::path::Path) -> Result<(), CoreError> {
        let Some(bytes) = crate::persist::read_durable(path)? else {
            return Ok(());
        };
        let text = String::from_utf8(bytes).map_err(|err| CoreError::new(err.to_string()))?;
        self.foundation.settings.apply_user_text(&text)?;
        self.foundation.layout_locked =
            self.foundation.settings.get("layout.locked") == Some("true");
        self.messages = crate::editor_language::messages_for_store(&self.foundation.settings);
        Ok(())
    }

    pub fn save_panel_state(&self, path: &std::path::Path) -> Result<(), CoreError> {
        crate::persist::atomic_write(path, &self.panel_state_bytes())
    }

    pub fn load_panel_state(&mut self, path: &std::path::Path) -> Result<(), CoreError> {
        let Some(bytes) = crate::persist::read_durable(path)? else {
            return Ok(());
        };
        let text = String::from_utf8(bytes).map_err(|err| CoreError::new(err.to_string()))?;
        let value = seal_sealtext::parse(&text)?;
        let Some(map) = value.as_map() else {
            return Ok(());
        };
        const KNOWN: &[&str] = &[
            "schema_version",
            "view",
            "tool",
            "snap",
            "grid",
            "selection",
            "expanded",
            "outliner",
            "scroll",
            "theme",
            "scale",
            "content",
            "console",
            "settings_query",
            "folder",
            "content_list",
            "viewport_bookmarks",
            "inspector",
            "inspector_sections",
            "viewport_pan_x",
            "viewport_pan_y",
            "viewport_zoom",
            "viewport_yaw",
            "viewport_pitch",
            "viewport_distance",
            "draft_viewport_pan_x",
            "draft_viewport_pan_y",
            "viewport_split",
        ];
        for (key, item) in map {
            if !KNOWN.contains(&key.as_str()) {
                if let Some(panel) = key.strip_prefix("inspector_lock:") {
                    if let seal_sealtext::SealValue::String(entity) = item {
                        self.models.inspector_locks.insert(panel.to_string(), entity.clone());
                    }
                    continue;
                }
                if key.ends_with(":query") {
                    if let seal_sealtext::SealValue::String(text) = item {
                        self.models.panel_states.insert(key.clone(), text.clone());
                    }
                    continue;
                }
                let text = match item {
                    seal_sealtext::SealValue::String(value) => value.clone(),
                    other => other.to_text(),
                };
                self.models.panel_states.insert(key.clone(), text);
                continue;
            }
            match (key.as_str(), item) {
                ("view", seal_sealtext::SealValue::String(text)) => {
                    self.models.viewport_view = text.clone()
                }
                ("tool", seal_sealtext::SealValue::String(text)) => {
                    self.models.viewport_tool = text.clone()
                }
                ("snap", seal_sealtext::SealValue::Bool(flag)) => self.models.snap = *flag,
                ("grid", seal_sealtext::SealValue::Bool(flag)) => self.models.viewport_grid = *flag,
                ("selection", seal_sealtext::SealValue::String(text)) => {
                    self.models.selection = Some(text.clone());
                    self.foundation.selection = vec![text.clone()];
                }
                ("selection", seal_sealtext::SealValue::Null) => {
                    self.models.selection = None;
                    self.foundation.selection.clear();
                }
                ("outliner", seal_sealtext::SealValue::String(text)) => {
                    self.models.outliner_query = text.clone()
                }
                ("scroll", seal_sealtext::SealValue::Number(text)) => {
                    if let Ok(scroll) = text.parse::<usize>() {
                        self.outliner_scroll = scroll;
                    }
                }
                ("content", seal_sealtext::SealValue::String(text)) => {
                    self.models.content_query = text.clone()
                }
                ("console", seal_sealtext::SealValue::String(text)) => {
                    self.models.console_query = text.clone()
                }
                ("settings_query", seal_sealtext::SealValue::String(text)) => {
                    self.models.settings_query = text.clone()
                }
                ("folder", seal_sealtext::SealValue::String(text)) => {
                    self.models.content_root = Some(std::path::PathBuf::from(text))
                }
                ("content_list", seal_sealtext::SealValue::Bool(flag)) => {
                    self.models.content_list = *flag
                }
                ("theme", seal_sealtext::SealValue::String(text)) => {
                    self.models.theme = text.clone()
                }
                ("scale", seal_sealtext::SealValue::Number(text)) => {
                    if let Ok(scale) = text.parse::<f32>() {
                        self.models.scale = scale;
                    }
                }
                ("expanded", seal_sealtext::SealValue::Array(items)) => {
                    self.models.expanded = items
                        .iter()
                        .filter_map(|item| match item {
                            seal_sealtext::SealValue::String(text) => Some(text.clone()),
                            _ => None,
                        })
                        .collect();
                }
                ("viewport_bookmarks", seal_sealtext::SealValue::Map(bookmarks)) => {
                    self.models.viewport_bookmarks.clear();
                    for (slot, entry) in &bookmarks.entries {
                        let Some(entry_map) = entry.as_map() else {
                            continue;
                        };
                        let mut view = String::new();
                        let mut selection = None;
                        let mut bookmark = crate::models::ViewportBookmark::default();
                        for (field, value) in entry_map {
                            match (field.as_str(), value) {
                                ("view", seal_sealtext::SealValue::String(text)) => {
                                    view = text.clone();
                                }
                                ("selection", seal_sealtext::SealValue::String(text)) => {
                                    selection = Some(text.clone());
                                }
                                ("selection", seal_sealtext::SealValue::Null) => {
                                    selection = None;
                                }
                                ("selection_ids", seal_sealtext::SealValue::Array(items)) => {
                                    bookmark.selection_ids = items
                                        .iter()
                                        .filter_map(|item| match item {
                                            seal_sealtext::SealValue::String(text) => {
                                                Some(text.clone())
                                            }
                                            _ => None,
                                        })
                                        .collect();
                                }
                                ("yaw", seal_sealtext::SealValue::Number(text)) => {
                                    bookmark.yaw = text.parse().unwrap_or(0.0);
                                }
                                ("pitch", seal_sealtext::SealValue::Number(text)) => {
                                    bookmark.pitch = text.parse().unwrap_or(0.0);
                                }
                                ("distance", seal_sealtext::SealValue::Number(text)) => {
                                    bookmark.distance = text.parse().unwrap_or(10.0);
                                }
                                ("pan_x", seal_sealtext::SealValue::Number(text)) => {
                                    bookmark.pan_x = text.parse().unwrap_or(0.0);
                                }
                                ("pan_y", seal_sealtext::SealValue::Number(text)) => {
                                    bookmark.pan_y = text.parse().unwrap_or(200.0);
                                }
                                ("zoom", seal_sealtext::SealValue::Number(text)) => {
                                    bookmark.zoom = text.parse().unwrap_or(1.0);
                                }
                                _ => {}
                            }
                        }
                        bookmark.view = view;
                        bookmark.selection = selection;
                        self.models.viewport_bookmarks.insert(slot.clone(), bookmark);
                    }
                }
                ("inspector", seal_sealtext::SealValue::String(text)) => {
                    self.models.inspector_query = text.clone();
                }
                ("inspector_sections", seal_sealtext::SealValue::Map(sections)) => {
                    self.models.inspector_sections.clear();
                    for (id, value) in &sections.entries {
                        if let seal_sealtext::SealValue::Bool(open) = value {
                            self.models.inspector_sections.insert(id.clone(), *open);
                        }
                    }
                }
                ("viewport_pan_x", seal_sealtext::SealValue::Number(text)) => {
                    if let Ok(pan) = text.parse() {
                        self.models.viewport_pan_x = pan;
                    }
                }
                ("viewport_pan_y", seal_sealtext::SealValue::Number(text)) => {
                    if let Ok(pan) = text.parse() {
                        self.models.viewport_pan_y = pan;
                    }
                }
                ("viewport_zoom", seal_sealtext::SealValue::Number(text)) => {
                    if let Ok(zoom) = text.parse::<f32>() {
                        self.models.viewport_zoom = zoom.clamp(0.25, 6.0);
                    }
                }
                ("viewport_split", seal_sealtext::SealValue::String(text)) => {
                    crate::viewport_split::restore_split_text(self, text);
                }
                ("viewport_split", _) => {
                    crate::viewport_split::fallback_single(&mut self.models);
                }
                (
                    "viewport_yaw"
                    | "viewport_pitch"
                    | "viewport_distance"
                    | "viewport_pan_x"
                    | "viewport_pan_y"
                    | "draft_viewport_pan_x"
                    | "draft_viewport_pan_y",
                    seal_sealtext::SealValue::String(text),
                ) => {
                    let key = match key.as_str() {
                        "viewport_yaw" => "viewport-yaw",
                        "viewport_pitch" => "viewport-pitch",
                        "viewport_distance" => "viewport-distance",
                        "viewport_pan_x" | "draft_viewport_pan_x" => "viewport-pan-x",
                        "viewport_pan_y" | "draft_viewport_pan_y" => "viewport-pan-y",
                        _ => continue,
                    };
                    self.store_loaded_draft(key, text);
                }
                _ => {}
            }
        }
        crate::viewport_overlays::sync_session_models(self);
        self.flatten_view_for_2d_scene();
        Ok(())
    }

    pub fn scratch_text(&self) -> String {
        self.documents
            .iter()
            .find(|doc| doc.id == "scratch")
            .map(|doc| doc.text.clone())
            .unwrap_or_default()
    }

    pub fn set_scratch_text(&mut self, text: &str) {
        let Some(doc) = self.documents.iter_mut().find(|doc| doc.id == "scratch") else {
            return;
        };
        let Some(op) = doc.replace_with(text) else {
            return;
        };
        let step = JournalStep::Edit(op);
        self.push_past(Past::Scratch);
        self.future.clear();
        self.write_journal(&step);
    }

    pub fn load_journal(&mut self, path: &std::path::Path) -> Result<(), CoreError> {
        self.journal_path = Some(path.to_path_buf());
        let steps = journal::read_steps(path)?;
        if steps.is_empty() {
            return Ok(());
        }
        if let Some(doc) = self.documents.iter_mut().find(|doc| doc.id == "scratch") {
            journal::replay_steps(doc, &steps);
        }
        Ok(())
    }

    pub fn layout_store_path(&self, user_layout: &std::path::Path) -> std::path::PathBuf {
        self.project
            .as_ref()
            .map(|project| project.root.join(".seal-editor").join("layout.seal"))
            .unwrap_or_else(|| user_layout.to_path_buf())
    }

    /// Editor state directory for the open project (settings, session, journals).
    pub fn project_state_dir(&self) -> Option<std::path::PathBuf> {
        self.recent_path.as_ref().and_then(|path| path.parent().map(|dir| dir.to_path_buf()))
    }

    /// Write the latest scene and undo journals before switching or exiting a project.
    pub fn flush_scene_journals(&mut self) -> Result<(), CoreError> {
        if self.awaiting_scene_recovery() || !self.scene_dirty {
            return Ok(());
        }
        self.write_journals()
    }

    pub(crate) fn attach_project(&mut self, project: OpenProject) -> Result<(), CoreError> {
        let root = project.root.clone();
        // A brush picked in the last project would paint with the wrong picture here.
        self.drafts.retain(|(key, _)| !TRANSIENT_DRAFTS.contains(&key.as_str()));
        // A recovered scene still waiting for an answer would block every later
        // autosave in the next project. Keep it (saved where it came from, with a
        // copy of its journal) before switching, so nothing is lost or stuck.
        if self.awaiting_scene_recovery() {
            self.keep_recovered_scene()?;
            let kept = self.scene_path().map(|p| p.display().to_string()).unwrap_or_default();
            self.notify(
                self.messages.format("recovery-kept-on-switch", &[("file", kept.as_str())]),
                false,
            );
        }
        // Never drop unsaved scene work when switching or reopening: write it first.
        if self.project.is_some() && self.scene_dirty {
            self.save_scene()?;
        }
        self.project_lock = None;
        match crate::persist::ProjectGuard::acquire(&root) {
            Ok(guard) => {
                if guard.replaced_stale {
                    self.notice = Some(self.messages.get("project-lock-stale"));
                }
                self.project_lock = Some(guard);
            }
            Err(err) => {
                if let Some(owner) = err.message.strip_prefix("project-open-elsewhere\t") {
                    let mut parts = owner.split('\t');
                    let machine = parts.next().unwrap_or("unknown");
                    let pid = parts.next().unwrap_or("0");
                    let body = self
                        .messages
                        .format("project-open-elsewhere", &[("machine", machine), ("pid", pid)]);
                    self.foundation.modal = Some(Modal {
                        kind: ModalKind::ProjectElsewhere,
                        title: self.messages.get("modal-project-elsewhere-title"),
                        body,
                        confirm: self.messages.get("modal-project-elsewhere-focus"),
                        alternate: String::new(),
                        cancel: self.messages.get("modal-project-elsewhere-dismiss"),
                    });
                    return Ok(());
                }
                return Err(err);
            }
        }
        if let Some(path) = &self.recent_path {
            project::touch_recent(path, &root)?;
        }
        let layout_path = root.join(".seal-editor").join("layout.seal");
        self.layout = if layout_path.exists() {
            crate::workspace::WorkspaceFile::load(&layout_path).layout()
        } else {
            Layout::default()
        };
        self.layout_epoch = self.layout_epoch.saturating_add(1);
        self.documents = vec![TextDocument::new("scratch")];
        let journal = root.join(".seal-editor").join("journal").join("scratch.journal");
        self.journal_path = Some(journal.clone());
        let read_only = project.read_only;
        self.project = Some(project);
        self.agent_load_log();
        // Each project shows its own scene; nothing carries over from the previous one.
        let remembered = std::fs::read_to_string(root.join(".seal-editor").join("open-scene.txt"))
            .ok()
            .map(|rel| root.join(rel.trim()))
            .filter(|path| path.is_file() && !path.ends_with("scene.seal"));
        let mut scene = remembered.clone().unwrap_or_else(|| root.join("scene.seal"));
        if read_only {
            if let Some(start) = crate::package::packaged_start_scene(&root) {
                let pick = root.join(&start);
                if pick.is_file() {
                    scene = pick;
                }
            } else if let Ok(build) = crate::build_settings::read_build_scenes(&root) {
                if let Some(first) = build.into_iter().next() {
                    let pick = root.join(&first);
                    if pick.is_file() {
                        scene = pick;
                    }
                }
            }
        }
        self.world = match std::fs::read_to_string(&scene).map(|text| {
            seal_world::object_files::expand(&scene, &text).unwrap_or(text)
        }) {
            Ok(text) => match World::from_text(&text) {
                Ok(world) => world,
                Err(err) => {
                    self.notify(
                        format!("{} could not be read: {err}", file_name_of(Some(scene.as_path()))),
                        true,
                    );
                    World::starter()
                }
            },
            Err(_) => World::starter(),
        };
        self.refresh_prefabs();
        self.adopt_world();
        self.flatten_view_for_2d_scene();
        self.models.selection = None;
        self.foundation.selection.clear();
        self.past.clear();
        self.future.clear();
        self.sync_undo_baseline();
        let steps = journal::read_steps(&journal)?;
        if let Some(doc) = self.documents.iter_mut().find(|doc| doc.id == "scratch") {
            journal::replay_steps(doc, &steps);
        }
        self.scene_override = remembered;
        self.scene_dirty = false;
        crate::content::open_content_folder(self, &root);
        let names = crate::inspector_fields::load_collision_layer_names(&root);
        let _ = self
            .foundation
            .settings
            .set_project(crate::inspector_fields::COLLISION_LAYER_SETTING, &names);
        let _ = self.models.set_setting(
            crate::inspector_fields::COLLISION_LAYER_SETTING,
            &names,
            "project",
        );
        let sort_names = crate::inspector_fields::load_sorting_layer_names(&root);
        let _ = self
            .foundation
            .settings
            .set_project(crate::inspector_fields::SORTING_LAYER_SETTING, &sort_names);
        let _ = self.models.set_setting(
            crate::inspector_fields::SORTING_LAYER_SETTING,
            &sort_names,
            "project",
        );
        if let Some(index) = self.keymap_path.as_ref().and_then(|p| p.parent()) {
            let active = crate::keymap_profiles::ProfileIndex::load(
                &crate::keymap_profiles::profile_index_path(index),
            );
            self.models.keymap_profile_name = active.active;
        }
        for (id, value) in crate::window_game::load_fields(&root) {
            let _ = self.foundation.settings.set_project(&id, &value);
            let _ = self.models.set_setting(&id, &value, "project");
        }
        for (id, value) in crate::render_quality::load_fields(&root) {
            let _ = self.foundation.settings.set_project(&id, &value);
            let _ = self.models.set_setting(&id, &value, "project");
        }
        let dimension = crate::project_dimension::load(&root);
        let _ = self.foundation.settings.set_project(crate::project_dimension::SETTING, &dimension);
        let _ = self.models.set_setting(crate::project_dimension::SETTING, &dimension, "project");
        self.models.platform_overrides = crate::platform_overrides::load(&root);
        self.models.settings_preview_platform = crate::platform_overrides::host_platform().into();
        crate::render_quality::sync_world_glow(self);
        crate::import_presets::load_for_project(self);
        crate::naming_rules::load_for_project(self);
        crate::asset_index::ensure_index_on_open(self);
        crate::tutorial_hooks::on_project_open(self);
        if read_only {
            self.notify(self.messages.get("project-read-only"), false);
        } else {
            let name = self.project.as_ref().map(|p| p.name.clone()).unwrap_or_default();
            seal_log::info!("project {name}");
            let text = self.messages.format("project-opened", &[("name", name.as_str())]);
            self.notify(text, false);
        }
        Ok(())
    }

    fn write_journal(&self, step: &JournalStep) {
        if let Some(path) = &self.journal_path {
            if let Err(err) = append_step(path, step) {
                seal_log::warn!("journal write failed: {err}");
            }
        }
    }

    fn register_commands(&mut self) -> Result<(), CoreError> {
        let specs = [
            ("file.quit", "command-file-quit", "file", quit as CommandFn),
            ("file.new-project", "command-file-new-project", "file", new_project),
            ("file.open-project", "command-file-open-project", "file", open_project_cmd),
            ("file.close-project", "command-file-close-project", "file", close_project),
            ("file.repair-project", "command-file-repair-project", "file", repair_project_cmd),
            ("edit.undo", "command-edit-undo", "edit", undo_active),
            ("edit.redo", "command-edit-redo", "edit", redo_active),
            ("undo.history_jump", "command-undo-history-jump", "edit", undo_history_jump),
            ("edit.discard", "command-edit-discard", "edit", discard_scratch),
            ("view.command-palette", "command-view-palette", "view", open_palette),
            ("window.save-layout", "command-window-save-layout", "window", save_layout),
            ("window.reset-layout", "command-window-reset-layout", "window", reset_layout),
            ("window.apply-layout", "command-window-apply-layout", "window", apply_named_layout),
            ("help.about", "command-help-about", "help", about),
            ("keymap.rebind", "command-keymap-rebind", "tools", rebind),
        ];
        for (id, title_key, category, handler) in specs {
            self.commands.register(
                CommandSpec::new(id, title_key, category),
                always_enabled,
                handler,
            )?;
        }
        Ok(())
    }

    pub fn dispatch(&mut self, id: &str, args: &CommandArgs) -> Result<(), CoreError> {
        let Some(canonical) = self.commands.canonical(id) else {
            return Err(CoreError::new(format!("unknown command '{id}'")));
        };
        let Some(spec) = self.commands.get(&canonical).cloned() else {
            return Err(CoreError::new(format!("unknown command '{id}'")));
        };
        if !self.requirements_met(&spec) || !self.commands.is_enabled(&canonical, self) {
            return Err(CoreError::new(self.messages.format("command-disabled", &[("id", id)])));
        }
        if canonical != "viewport.move" {
            seal_log::debug!("command {canonical} {}", args.redacted());
        }
        let Some(handler) = self.commands.handler(&canonical) else {
            return Err(CoreError::new(format!("unknown command '{id}'")));
        };
        let creates_asset = canonical == "graph.new"
            || (canonical.starts_with("asset.") && canonical.ends_with("_new"));
        self.pending_undo_command = Some(canonical.clone());
        self.note_recent(&canonical);
        self.foundation.events.push(crate::foundation::EditorEvent::Command { id: canonical });
        let result = handler(self, args);
        self.pending_undo_command = None;
        // New assets open in the Asset Editor; check the new name against the project's naming rules.
        if result.is_ok() && creates_asset {
            if let Some(rel) = self.draft(crate::asset_editor::ASSET_DRAFT) {
                crate::naming_rules::note_violation(self, &rel);
            }
        }
        result
    }

    pub fn invoke_json(&mut self, text: &str) -> Result<String, CoreError> {
        let value: serde_json::Value =
            serde_json::from_str(text).map_err(|err| CoreError::new(err.to_string()))?;
        let id = value
            .get("id")
            .and_then(|item| item.as_str())
            .ok_or_else(|| CoreError::new("command json needs an id"))?;
        let args = match value.get("args") {
            Some(args) => CommandArgs::from_json_object(&args.to_string())?,
            None => CommandArgs::default(),
        };
        let id = id.to_string();
        self.dispatch(&id, &args)?;
        Ok(format!("{{\"ok\":true,\"id\":\"{}\"}}", escape(&id)))
    }

    pub fn catalog_json(&self) -> String {
        self.commands.catalog_json(|key| self.messages.get(key))
    }

    pub fn palette_hits(&self) -> Vec<PaletteHit> {
        let (mode, query) = PaletteMode::split(&self.palette_query);
        match mode {
            PaletteMode::Commands => self.ranked_commands(query),
            PaletteMode::Help => {
                let mut hits = self.ranked_commands(query);
                hits.retain(|hit| {
                    self.commands.get(&hit.id).is_some_and(|spec| spec.category == "help")
                });
                hits
            }
            PaletteMode::Panels => list_hits(query, &self.foundation.open_panels),
            PaletteMode::RecentProjects => {
                let recent = self
                    .recent_path
                    .as_ref()
                    .and_then(|path| crate::project::read_recent(path).ok())
                    .unwrap_or_default();
                list_hits(query, &recent)
            }
        }
    }

    fn ranked_commands(&self, query: &str) -> Vec<PaletteHit> {
        let mut hits = palette_hits(
            &self.commands,
            query,
            &self.recent_commands,
            |key| self.messages.get(key),
            |id| {
                let shortcut = self.keymap.shortcut(id);
                if shortcut.is_empty() {
                    self.keymap.chord_for(id).map(Chord::label).unwrap_or_default()
                } else {
                    shortcut
                }
            },
        );
        for hit in &mut hits {
            if let Some((_, count)) = self.command_hits.iter().find(|(id, _)| id == &hit.id) {
                hit.score += *count as i32;
            }
        }
        hits.sort_by(|left, right| {
            right.score.cmp(&left.score).then_with(|| left.title.cmp(&right.title))
        });
        hits
    }

    pub fn can_undo(&self) -> bool {
        !self.past.is_empty() || self.documents.iter().any(|doc| doc.undo_len() > 0)
    }

    pub fn can_redo(&self) -> bool {
        !self.future.is_empty() || self.documents.iter().any(|doc| doc.redo_len() > 0)
    }

    /// Name of the edit that Undo would reverse, when one is on the stack.
    pub fn undo_action_name(&self) -> Option<String> {
        self.past.last().map(|item| self.past_action_label(item))
    }

    /// Name of the edit that Redo would apply again, when one is waiting.
    pub fn redo_action_name(&self) -> Option<String> {
        self.future.last().map(|item| self.past_action_label(item))
    }

    pub fn past_action_label(&self, item: &Past) -> String {
        if let Past::Pose { label, .. } = item {
            if !label.is_empty() {
                return label.clone();
            }
        }
        if let Past::Scene { label, .. } = item {
            if !label.is_empty() {
                return label.clone();
            }
        }
        match item {
            Past::Rename { next, .. } => {
                self.messages.format("undo-act-rename", &[("name", next.as_str())])
            }
            Past::Reparent { id, .. } => self
                .messages
                .format("undo-act-reparent", &[("name", self.entity_display_name(id).as_str())]),
            Past::Field { id, field, .. } => self.messages.format(
                "undo-act-field",
                &[("name", self.entity_display_name(id).as_str()), ("field", field.as_str())],
            ),
            _ => self.messages.get(history_name(item)),
        }
    }

    fn entity_display_name(&self, id: &str) -> String {
        self.world
            .entities
            .iter()
            .find(|entity| entity.id == id)
            .map(|entity| entity.name.clone())
            .unwrap_or_else(|| id.to_string())
    }

    pub fn menu_ids(&self, category: &str) -> Vec<String> {
        let mut ids: Vec<String> = self
            .commands
            .iter()
            .filter(|spec| {
                spec.in_menu && spec.alias_of.is_none() && spec.menu_bucket() == category
            })
            .map(|spec| spec.id.clone())
            .collect();
        ids.sort_by(|left, right| {
            let title = |id: &str| {
                self.commands
                    .get(id)
                    .map(|spec| self.messages.get(&spec.title_key))
                    .unwrap_or_else(|| id.to_string())
            };
            title(left).cmp(&title(right))
        });
        ids
    }

    pub fn categories_cover_every_command(&self) -> bool {
        self.commands.iter().all(|spec| {
            if !MENU_CATEGORIES.contains(&spec.menu_bucket()) {
                return false;
            }
            spec.alias_of.is_some()
                || !spec.in_menu
                || self.menu_ids(spec.menu_bucket()).iter().any(|id| id == &spec.id)
        })
    }

    pub fn console_max_entries(&self) -> usize {
        self.foundation
            .settings
            .get("console.max_entries")
            .and_then(|text| text.parse().ok())
            .unwrap_or(5000)
            .max(1)
    }

    pub fn console_filtered_lines(&self) -> Vec<crate::models::LogLine> {
        let records = crate::models::log_records();
        self.models
            .filtered_logs(&records, self.console_max_entries())
            .into_iter()
            .cloned()
            .collect()
    }

    pub fn console_selected_index(&self, total: usize) -> usize {
        if total == 0 {
            return 0;
        }
        self.draft(crate::models::CONSOLE_ROW_DRAFT)
            .and_then(|text| text.parse().ok())
            .unwrap_or(total.saturating_sub(1))
            .min(total - 1)
    }

    fn note_recent(&mut self, id: &str) {
        self.recent_commands.retain(|existing| existing != id);
        self.recent_commands.insert(0, id.to_string());
        self.recent_commands.truncate(12);
        if let Err(err) = self.save_memory() {
            self.notice = Some(err.to_string());
        }
        if let Some(slot) = self.command_hits.iter_mut().find(|(name, _)| name == id) {
            slot.1 = slot.1.saturating_add(1);
        } else {
            self.command_hits.push((id.to_string(), 1));
        }
    }

    pub fn load_user_keymap(&mut self, path: &std::path::Path) -> Result<(), CoreError> {
        self.keymap_path = Some(path.to_path_buf());
        if path.exists() {
            let user = Keymap::load(path)?;
            self.keymap.apply_user(&user);
        }
        Ok(())
    }

    pub fn save_keymap(&self) -> Result<(), CoreError> {
        if let Some(path) = &self.keymap_path {
            self.keymap.save(path)?;
        }
        Ok(())
    }
}

type CommandFn = fn(&mut EditorSession, &CommandArgs) -> Result<(), CoreError>;

fn new_project(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let Some(path) = args.get("path") else {
        crate::wizard::open_wizard(session);
        return Ok(());
    };
    let name = args.get("name").unwrap_or("project");
    let init_git = matches!(args.get("git"), Some("1" | "true"));
    let git_lfs = args.get("lfs").is_none_or(|v| !matches!(v, "0" | "false")) && init_git;
    let confirm = matches!(args.get("confirm_nonempty"), Some("1" | "true"));
    let options = project::CreateProjectOptions { init_git, git_lfs, confirm_nonempty: confirm };
    let project = project::create_project_with_options(std::path::Path::new(path), name, &options)?;
    session.wizard_open = false;
    session.attach_project(project)
}

fn open_project_cmd(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let Some(path) = args.get("path") else {
        session.host_action = Some(HostAction::PickFolder { create: false });
        return Ok(());
    };
    let folder = std::path::Path::new(path);
    let project = match project::open_project(folder) {
        Ok(project) => project,
        // A copied folder still carries its source's id: give the copy its own.
        Err(err) => match project::copy_source(folder) {
            Some(source) if !project::folder_is_read_only(folder) => {
                let project = project::repair_project(folder)?;
                let note =
                    session.messages.format("project-copy-new-id", &[("path", source.as_str())]);
                session.attach_project(project)?;
                session.notify(note, false);
                return Ok(());
            }
            _ => return Err(err),
        },
    };
    session.attach_project(project)
}

fn repair_project_cmd(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let path = args.get("path").ok_or_else(|| CoreError::new("repair project needs a path"))?;
    let project = project::repair_project(std::path::Path::new(path))?;
    session.attach_project(project)
}

fn close_project(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    // Closing never drops unsaved scene work: write it first.
    if session.project.is_some() && session.scene_dirty {
        session.save_scene()?;
    }
    session.project_lock = None;
    session.project = None;
    session.layout = Layout::default();
    session.layout_epoch = session.layout_epoch.saturating_add(1);
    session.documents = vec![TextDocument::new("scratch")];
    session.journal_path = None;
    Ok(())
}

fn quit(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    session.begin_close();
    Ok(())
}

fn history_name(item: &Past) -> &'static str {
    match item {
        Past::Scratch => "menu-act-text",
        Past::Rename { .. } => "menu-act-rename",
        Past::Reparent { .. } => "menu-act-reparent",
        Past::Field { .. } => "menu-act-field",
        Past::Pose { .. } => "menu-act-transform",
        Past::Scene { .. } => "menu-act-scene",
        Past::File { .. } => "menu-act-effect",
        Past::ImportBatch { .. } => "menu-act-import-preset",
        Past::ContentPathChange { .. } => "menu-act-content-path",
        Past::MergeDuplicates { .. } => "command-asset-merge-duplicates",
    }
}

fn undo_history_jump(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let Some(index) = args.get("index").and_then(|text| text.parse::<usize>().ok()) else {
        return Ok(());
    };
    session.jump_undo_history(index)
}

fn undo_active(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    if session.foundation.focused_panel_id == "timeline" {
        if let Some(doc) = session.timeline.as_mut() {
            if doc.undo() {
                return Ok(());
            }
        }
    }
    if let Some(doc) = session.graph_in_focus() {
        doc.undo();
        return Ok(());
    }
    let Some(item) = session.past.pop() else {
        let undone = session
            .documents
            .iter_mut()
            .find(|doc| doc.id == "scratch")
            .is_some_and(|doc| doc.undo());
        if undone {
            session.write_journal(&JournalStep::Undo);
        } else {
            session.notice = Some(session.messages.get("notice-nothing-to-undo"));
        }
        return Ok(());
    };
    match item {
        Past::Scratch => {
            let undone = session
                .documents
                .iter_mut()
                .find(|doc| doc.id == "scratch")
                .is_some_and(|doc| doc.undo());
            if undone {
                session.write_journal(&JournalStep::Undo);
                session.future.push(Past::Scratch);
            }
        }
        other => undo_model(session, other),
    }
    Ok(())
}

fn redo_active(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    if session.foundation.focused_panel_id == "timeline" {
        if let Some(doc) = session.timeline.as_mut() {
            if doc.redo() {
                return Ok(());
            }
        }
    }
    if let Some(doc) = session.graph_in_focus() {
        doc.redo();
        return Ok(());
    }
    let Some(item) = session.future.pop() else {
        let redone = session
            .documents
            .iter_mut()
            .find(|doc| doc.id == "scratch")
            .is_some_and(|doc| doc.redo());
        if redone {
            session.write_journal(&JournalStep::Redo);
        } else {
            session.notice = Some(session.messages.get("notice-nothing-to-redo"));
        }
        return Ok(());
    };
    match item {
        Past::Scratch => {
            let redone = session
                .documents
                .iter_mut()
                .find(|doc| doc.id == "scratch")
                .is_some_and(|doc| doc.redo());
            if redone {
                session.write_journal(&JournalStep::Redo);
                session.push_past(Past::Scratch);
            }
        }
        other => redo_model(session, other),
    }
    Ok(())
}

fn discard_scratch(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    if let Some(doc) = session.documents.iter_mut().find(|doc| doc.id == "scratch") {
        doc.text = doc.saved.clone();
        doc.clear_history();
    }
    if let Some(path) = &session.journal_path {
        journal::discard_journal(path)?;
    }
    Ok(())
}

fn open_palette(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    session.palette_open = true;
    session.palette_query.clear();
    session.palette_index = 0;
    Ok(())
}

fn save_layout(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    session.request_save = true;
    Ok(())
}

fn reset_layout(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    if session.foundation.layout_locked {
        return Err(CoreError::new("layout is locked"));
    }
    session.foundation.modal = Some(crate::foundation::Modal {
        kind: crate::foundation::ModalKind::ResetLayout,
        title: session.messages.get("modal-reset-layout-title"),
        body: session.messages.get("modal-reset-layout-body"),
        confirm: session.messages.get("modal-reset-layout-confirm"),
        alternate: String::new(),
        cancel: session.messages.get("modal-close-cancel"),
    });
    Ok(())
}

pub fn apply_layout_reset(session: &mut EditorSession) {
    session.foundation.previous_layout = Some(session.layout.clone());
    session.layout = Layout::default();
    session.layout_epoch = session.layout_epoch.saturating_add(1);
    apply_default_workspace(session);
    session.foundation.events.push(crate::foundation::EditorEvent::Layout);
}

pub fn apply_default_workspace(session: &mut EditorSession) {
    use crate::layout::DEFAULT_SIDEBAR_PINS;
    session.foundation.sidebar_pins =
        DEFAULT_SIDEBAR_PINS.iter().map(|id| (*id).to_string()).collect();
    session.models.expanded =
        vec!["root".into(), "geo".into(), "lighting".into(), "gameplay".into()];
}

fn apply_named_layout(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    if session.foundation.layout_locked {
        return Err(CoreError::new("layout is locked"));
    }
    let name = args.get("name").unwrap_or("default");
    let Some(layout) = Layout::builtin(name) else {
        return Err(CoreError::new(format!("unknown layout '{name}'")));
    };
    session.foundation.previous_layout = Some(session.layout.clone());
    session.layout = layout;
    session.layout_epoch = session.layout_epoch.saturating_add(1);
    Ok(())
}

fn list_hits(query: &str, items: &[String]) -> Vec<PaletteHit> {
    let needle = query.to_lowercase();
    items
        .iter()
        .filter(|item| needle.is_empty() || item.to_lowercase().contains(&needle))
        .map(|item| PaletteHit {
            id: item.clone(),
            title: item.clone(),
            shortcut: String::new(),
            score: 1,
        })
        .collect()
}

fn about(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    session.foundation.modal = Some(crate::foundation::Modal {
        kind: crate::foundation::ModalKind::About,
        title: session.messages.get("about-title"),
        body: session
            .messages
            .format("about-body", &[("version", crate::version::ENGINE_VERSION_FULL)]),
        confirm: session.messages.get("about-ok"),
        alternate: String::new(),
        cancel: String::new(),
    });
    Ok(())
}

fn rebind(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let command = args.get("command").ok_or_else(|| CoreError::new("rebind needs a command"))?;
    let chord =
        Chord::parse(args.get("chord").ok_or_else(|| CoreError::new("rebind needs a chord"))?)?;
    if session.commands.get(command).is_none() {
        return Err(CoreError::new(format!("unknown command '{command}'")));
    }
    let command = command.to_string();
    session.keymap.bind(&command, chord)?;
    session.save_keymap()?;
    Ok(())
}

fn parse_unit(value: &str, fallback: f32) -> f32 {
    value.parse::<f32>().unwrap_or(fallback).clamp(0.0, 1.0)
}

fn format_axis(value: f32) -> String {
    let text = format!("{value:.4}");
    let trimmed = text.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() || trimmed == "-" {
        "0".into()
    } else {
        trimmed.into()
    }
}

fn file_name_of(path: Option<&std::path::Path>) -> String {
    path.and_then(|path| path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn kept_journal_path(path: &std::path::Path) -> Result<std::path::PathBuf, CoreError> {
    let name = path.file_name().ok_or_else(|| CoreError::new("journal path has no file name"))?;
    let mut kept = name.to_os_string();
    kept.push(".kept");
    Ok(path.with_file_name(kept))
}

fn remove_if_present(path: Option<&std::path::Path>) -> Result<(), CoreError> {
    if let Some(path) = path {
        if path.exists() {
            std::fs::remove_file(path)?;
        }
    }
    Ok(())
}

fn note_startup(session: &mut EditorSession, result: Result<(), CoreError>) {
    if let Err(err) = result {
        seal_log::warn!("startup: {err}");
        if session.notice.is_none() {
            session.notice = Some(err.to_string());
        }
    }
}

/// Idle debounce or the maximum gap, whichever comes first. Debounce is at least 500 ms.
pub fn autosave_due(
    idle_ms: u128,
    dirty_ms: u128,
    debounce_ms: u128,
    max_interval_ms: u128,
) -> bool {
    let debounce = debounce_ms.max(500);
    let cap = max_interval_ms.max(debounce);
    idle_ms >= debounce || dirty_ms >= cap
}

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_menu_command_runs_and_the_palette_finds_it() {
        let mut session = EditorSession::new(Layout::default());
        assert!(session.categories_cover_every_command());
        session.dispatch("view.command-palette", &CommandArgs::default()).unwrap();
        assert!(session.palette_open);
        session.palette_query = "pal".into();
        let hits = session.palette_hits();
        assert_eq!(hits[0].id, "view.command-palette");
        session.dispatch("window.reset-layout", &CommandArgs::default()).unwrap();
        session.choose_modal(crate::foundation::ModalChoice::Confirm);
        assert!(session.layout_epoch >= 1);
        session.dispatch("help.about", &CommandArgs::default()).unwrap();
        assert!(session.foundation.modal.is_some());
        session.dispatch("file.quit", &CommandArgs::default()).unwrap();
        assert!(session.request_close);
    }

    #[test]
    fn rebind_command_persists_and_json_runs_the_same_command() {
        let dir = std::env::temp_dir().join(format!("seal-session-keys-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut session = EditorSession::new(Layout::default());
        session.load_user_keymap(&dir.join("keymap.seal")).unwrap();
        session
            .dispatch(
                "keymap.rebind",
                &CommandArgs::new(&[("command", "file.quit"), ("chord", "ctrl+shift+q")]),
            )
            .unwrap();
        let mut again = EditorSession::new(Layout::default());
        again.load_user_keymap(&dir.join("keymap.seal")).unwrap();
        assert_eq!(
            again.keymap.command_for(&Chord::parse("ctrl+shift+q").unwrap()),
            Some("file.quit")
        );
        let catalog = session.catalog_json();
        assert!(catalog.contains("file.quit"));
        session.palette_open = false;
        session.invoke_json("{\"id\":\"view.command-palette\"}").unwrap();
        assert!(session.palette_open);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scratch_edits_survive_a_restart_and_undo() {
        let dir = std::env::temp_dir().join(format!("seal-scratch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("scratch.journal");
        let mut session = EditorSession::new(Layout::default());
        session.load_journal(&path).unwrap();
        session.set_scratch_text("ab");
        session.dispatch("edit.undo", &CommandArgs::default()).unwrap();
        assert_eq!(session.scratch_text(), "");
        let mut restored = EditorSession::new(Layout::default());
        restored.load_journal(&path).unwrap();
        assert_eq!(restored.scratch_text(), "");
        restored.dispatch("edit.redo", &CommandArgs::default()).unwrap();
        assert_eq!(restored.scratch_text(), "ab");
        restored.dispatch("edit.discard", &CommandArgs::default()).unwrap();
        assert_eq!(restored.scratch_text(), "");
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn project_open_close_and_repair_go_through_commands() {
        let dir = std::env::temp_dir().join(format!("seal-proj-cmd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let folder = dir.join("game");
        let mut session = EditorSession::new(Layout::default());
        session.recent_path = Some(dir.join("recent.seal"));
        session
            .dispatch(
                "file.new-project",
                &CommandArgs::new(&[("path", folder.to_str().unwrap()), ("name", "Game")]),
            )
            .unwrap();
        assert_eq!(session.project.as_ref().map(|project| project.name.as_str()), Some("Game"));
        session.project = None;
        session
            .dispatch("file.open-project", &CommandArgs::new(&[("path", folder.to_str().unwrap())]))
            .unwrap();
        assert!(session.project.is_some());
        std::fs::write(folder.join("project.seal"), b"!!!").unwrap();
        let error = session.dispatch(
            "file.open-project",
            &CommandArgs::new(&[("path", folder.to_str().unwrap())]),
        );
        assert!(error.is_err());
        session
            .dispatch(
                "file.repair-project",
                &CommandArgs::new(&[("path", folder.to_str().unwrap())]),
            )
            .unwrap();
        assert!(session.project.is_some());
        session.dispatch("file.close-project", &CommandArgs::default()).unwrap();
        assert!(session.project.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn play_runs_object_scripts_from_the_project_and_logs_them() {
        let dir = std::env::temp_dir().join(format!("seal-proj-script-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let folder = dir.join("game");
        let mut session = EditorSession::new(Layout::default());
        session.recent_path = Some(dir.join("recent.seal"));
        session
            .dispatch(
                "file.new-project",
                &CommandArgs::new(&[("path", folder.to_str().unwrap()), ("name", "Game")]),
            )
            .unwrap();
        std::fs::create_dir_all(folder.join("logic")).unwrap();
        std::fs::write(
            folder.join("logic/hello.graph"),
            "schema_version = 1\nseal_graph = \"hello\"\nvars = []\nnodes = [\n  { id = 1, kind = \"event\", name = \"start\" },\n  { id = 2, kind = \"log\", defaults = { message = \"hello from a script\" } },\n]\nlinks = [\"1.then -> 2.in\"]\n",
        )
        .unwrap();
        session.world.entities[0].script = "logic/hello.graph".into();
        session.dispatch("play.start", &CommandArgs::default()).unwrap();
        assert!(session.world.play_log.is_empty(), "forwarded to the Console");
        let lines = crate::models::log_records();
        assert!(
            lines.iter().any(|l| format!("{l:?}").contains("hello from a script")),
            "{lines:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn console_save_copy_and_latest_use_filtered_lines() {
        let mut session = EditorSession::new(Layout::default());
        seal_log::info!("alpha");
        seal_log::warn!("beta");
        session.dispatch("console.filter", &CommandArgs::new(&[("level", "warn")])).unwrap();
        session.set_draft(crate::models::CONSOLE_ROW_DRAFT, "0").unwrap();
        session.dispatch("console.copy_selection", &CommandArgs::default()).unwrap();
        assert!(session.foundation.clipboard.contains("beta"));
        assert!(!session.foundation.clipboard.contains("alpha"));
        session.dispatch("console.jump_latest", &CommandArgs::default()).unwrap();
        assert_eq!(session.draft(crate::models::CONSOLE_ROW_DRAFT).unwrap(), "0");
    }

    #[test]
    fn edit_menu_lists_select_children_and_rename_without_shortcuts() {
        let session = EditorSession::new(Layout::default());
        let edit = session.menu_ids("edit");
        assert!(edit.iter().any(|id| id == "edit.select-children"));
        assert!(edit.iter().any(|id| id == "edit.rename"));
        assert!(edit.iter().any(|id| id == "outliner.rename"));
        assert!(session.keymap.shortcut("edit.select-children").is_empty());
        assert!(!session.commands.iter().any(|spec| spec.id == "play.from-here"));
        assert!(session.commands.get("play.step").is_some());
    }

    #[test]
    fn copy_diagnostics_bundles_redacted_system_fields() {
        let mut session = EditorSession::new(Layout::default());
        seal_log::info!("diag-line");
        session.dispatch("app.copy_diagnostics", &CommandArgs::default()).unwrap();
        let text = &session.foundation.clipboard;
        assert!(text.contains(crate::version::ENGINE_VERSION_FULL));
        assert!(text.contains("core-panels"));
        assert!(text.contains("diag-line"));
        assert!(text.contains("gpu"));
    }

    #[test]
    fn importing_a_picture_copies_it_and_places_a_numbered_sprite() {
        let dir = std::env::temp_dir().join(format!("seal-import-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let folder = dir.join("Game");
        let art = dir.join("coin.png");
        std::fs::write(&art, b"not really a png").unwrap();
        let mut session = EditorSession::new(Layout::default());
        session.recent_path = Some(dir.join("recent.seal"));
        session
            .dispatch(
                "file.new-project",
                &CommandArgs::new(&[("path", folder.to_str().unwrap()), ("name", "Game")]),
            )
            .unwrap();
        let before = session.world.entities.len();
        session
            .dispatch("asset.import", &CommandArgs::new(&[("path", art.to_str().unwrap())]))
            .unwrap();
        crate::import_pipeline::pump_all(&mut session).unwrap();
        assert_eq!(session.world.entities.len(), before, "a plain import only copies the file");
        let place = |session: &mut EditorSession| {
            session
                .dispatch(
                    "asset.import",
                    &CommandArgs::new(&[("path", art.to_str().unwrap()), ("place", "1")]),
                )
                .unwrap();
        };
        place(&mut session);
        crate::import_pipeline::pump_all(&mut session).unwrap();
        place(&mut session);
        crate::import_pipeline::pump_all(&mut session).unwrap();
        assert!(folder.join("sprites/coin.png").is_file());
        assert!(!folder.join("sprites/coin-2.png").exists(), "same bytes reuse the first copy");
        let sprites: Vec<_> =
            session.world.entities.iter().filter(|e| e.sprite == "sprites/coin.png").collect();
        assert_eq!(session.world.entities.len(), before + 2);
        assert_eq!(sprites.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["coin", "coin 2"]);
        session.dispatch("scene.add", &CommandArgs::new(&[("kind", "cube")])).unwrap();
        session.dispatch("scene.add", &CommandArgs::new(&[("kind", "cube")])).unwrap();
        let cubes: Vec<_> = session
            .world
            .entities
            .iter()
            .filter(|e| e.name.starts_with("Cube"))
            .map(|e| e.name.clone())
            .collect();
        assert_eq!(cubes, ["Cube", "Cube 2"]);
        session.dispatch("asset.import", &CommandArgs::default()).unwrap();
        assert!(matches!(session.host_action, Some(HostAction::ImportAsset)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn importing_a_folder_brings_in_every_known_file() {
        let dir = std::env::temp_dir().join(format!("seal-import-dir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let art = dir.join("art");
        std::fs::create_dir_all(&art).unwrap();
        for name in ["grass.png", "dirt.png", "sparkle.fx", "notes.txt"] {
            std::fs::write(art.join(name), name.as_bytes()).unwrap();
        }
        let folder = dir.join("Game");
        let mut session = EditorSession::new(Layout::default());
        session.recent_path = Some(dir.join("recent.seal"));
        session
            .dispatch(
                "file.new-project",
                &CommandArgs::new(&[("path", folder.to_str().unwrap()), ("name", "Game")]),
            )
            .unwrap();
        session
            .dispatch("asset.import", &CommandArgs::new(&[("path", art.to_str().unwrap())]))
            .unwrap();
        crate::import_pipeline::pump_all(&mut session).unwrap();
        assert!(folder.join("sprites/grass.png").is_file());
        assert!(folder.join("sprites/dirt.png").is_file());
        assert!(folder.join("fx/sparkle.fx").is_file());
        assert!(!folder.join("notes.txt").exists(), "unknown kinds stay behind");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn duplicate_leaves_only_the_copy_selected() {
        let mut session = EditorSession::new(Layout::default());
        session.dispatch("scene.add", &CommandArgs::new(&[("kind", "cube")])).unwrap();
        let original = session.primary_selection().unwrap();
        session.foundation.selection = vec![original.clone()];
        session.dispatch("edit.duplicate", &CommandArgs::default()).unwrap();
        let copy = session.primary_selection().unwrap();
        assert_ne!(copy, original);
        assert_eq!(session.foundation.selection, vec![copy]);
    }

    #[test]
    fn the_wizard_starts_in_a_new_empty_folder_named_after_the_project() {
        let mut session = EditorSession::new(Layout::default());
        crate::wizard::open_wizard(&mut session);
        let folder = crate::wizard::wizard_folder(&session);
        assert_eq!(folder.parent(), Some(crate::wizard::default_projects_dir().as_path()));
        assert!(!folder.exists());
        assert_eq!(crate::wizard::wizard_name(&session), folder.file_name().unwrap().to_str().unwrap());
    }

    #[test]
    fn the_wizard_creates_the_typed_folder_and_closes() {
        let dir = std::env::temp_dir().join(format!("seal-wizard-create-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut session = EditorSession::new(Layout::default());
        session.recent_path = Some(dir.join("recent.seal"));
        crate::wizard::open_wizard(&mut session);
        let folder = dir.join("projects").join("Hop2");
        session.set_draft(crate::wizard::DRAFT_FOLDER, folder.to_str().unwrap()).unwrap();
        session.set_draft(crate::wizard::DRAFT_NAME, "Hop2").unwrap();
        session.dispatch("wizard.create", &CommandArgs::default()).unwrap();
        assert!(folder.join("project.seal").is_file());
        assert_eq!(session.project.as_ref().map(|p| p.root.clone()), Some(folder.clone()));
        assert!(!session.wizard_open);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_wizard_applies_every_template_in_a_folder_it_just_checked() {
        let dir = std::env::temp_dir().join(format!("seal-wizard-tpl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut scenes = std::collections::BTreeMap::new();
        for template in crate::template_registry::PROJECT_TEMPLATES {
            let mut session = EditorSession::new(Layout::default());
            session.recent_path = Some(dir.join("recent.seal"));
            crate::wizard::open_wizard(&mut session);
            let folder = dir.join(template.id);
            std::fs::create_dir_all(&folder).unwrap();
            session.set_draft(crate::wizard::DRAFT_FOLDER, folder.to_str().unwrap()).unwrap();
            session.set_draft(crate::wizard::DRAFT_TEMPLATE, template.id).unwrap();
            // Showing the panel validates the folder; that check must not make it "not empty".
            let _ = crate::wizard::validate_wizard(&session.messages, "Game", &folder, false);
            session
                .dispatch("wizard.create", &CommandArgs::default())
                .unwrap_or_else(|e| panic!("{}: {e}", template.id));
            let files = std::fs::read_dir(&folder).unwrap().count();
            let scene = std::fs::read_to_string(folder.join("scene.seal")).unwrap_or_default();
            scenes.insert(template.id, (files, scene.len()));
        }
        // Starter templates write more than the empty 2D project does.
        let (empty_files, _) = scenes["2d"];
        for id in ["platformer", "topdown", "puzzle"] {
            assert!(scenes[id].0 > empty_files, "{id} wrote only {:?}", scenes[id]);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_path_asks_the_window_to_pick_one() {
        let mut session = EditorSession::new(Layout::default());
        session.dispatch("file.new-project", &CommandArgs::default()).unwrap();
        assert!(session.wizard_open);
        assert_eq!(session.reveal_panel.as_deref(), Some("wizard.project"));
        session.dispatch("file.open-project", &CommandArgs::default()).unwrap();
        assert!(matches!(session.host_action, Some(HostAction::PickFolder { create: false })));
        session.dispatch("app.restart", &CommandArgs::default()).unwrap();
        assert!(matches!(session.host_action, Some(HostAction::Restart { safe_mode: false })));
        assert!(!session.request_close);
        let dir = std::env::temp_dir().join(format!("seal-log-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("console.txt");
        seal_log::info!("hello-log");
        session
            .dispatch("console.save", &CommandArgs::new(&[("path", path.to_str().unwrap())]))
            .unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("hello-log"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_crash_before_save_keeps_the_scene_and_its_undo() {
        let dir = std::env::temp_dir().join(format!("seal-scene-journal-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut session = EditorSession::new(Layout::default());
        session.user_scene = Some(dir.join("scene.seal"));
        session.scene_journal = Some(dir.join("scene.journal"));
        session.undo_journal = Some(dir.join("undo.journal"));
        session.save_scene().unwrap();
        let before = session.world.entities.len();
        session.dispatch("scene.add", &CommandArgs::new(&[("kind", "cube")])).unwrap();
        assert!(session.world.entities.len() > before);
        assert!(dir.join("scene.journal").exists());
        let mut restored = EditorSession::new(Layout::default());
        restored.user_scene = Some(dir.join("scene.seal"));
        restored.scene_journal = Some(dir.join("scene.journal"));
        restored.undo_journal = Some(dir.join("undo.journal"));
        restored.load_scene(&dir.join("scene.seal")).unwrap();
        assert_eq!(restored.world.entities.len(), before);
        assert!(restored.recover_journals().unwrap());
        assert!(restored.world.entities.len() > before);
        restored.dispatch("edit.undo", &CommandArgs::default()).unwrap();
        assert_eq!(restored.world.entities.len(), before);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn opening_a_project_keeps_a_pending_recovery_and_autosave_resumes() {
        let dir = std::env::temp_dir().join(format!("seal-recover-switch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut session = EditorSession::new(Layout::default());
        session.user_scene = Some(dir.join("scene.seal"));
        session.scene_journal = Some(dir.join("scene.journal"));
        session.undo_journal = Some(dir.join("undo.journal"));
        session.save_scene().unwrap();
        session.dispatch("scene.add", &CommandArgs::new(&[("kind", "cube")])).unwrap();
        let crashed = session.world.entities.len();
        let mut restored = EditorSession::new(Layout::default());
        restored.user_scene = Some(dir.join("scene.seal"));
        restored.scene_journal = Some(dir.join("scene.journal"));
        restored.undo_journal = Some(dir.join("undo.journal"));
        restored.load_scene(&dir.join("scene.seal")).unwrap();
        assert!(restored.recover_journals().unwrap());
        restored.present_recovery(true, true).unwrap();
        assert!(restored.awaiting_scene_recovery());
        let project = crate::project::create_project(&dir.join("Fresh"), "Fresh").unwrap();
        let root = project.root.clone();
        restored
            .dispatch("file.open-project", &CommandArgs::new(&[("path", root.to_str().unwrap())]))
            .unwrap();
        // The recovered scene was written where it came from, not dropped.
        let kept = std::fs::read_to_string(dir.join("scene.seal")).unwrap();
        assert_eq!(seal_world::World::from_text(&kept).unwrap().entities.len(), crashed);
        assert!(!restored.awaiting_scene_recovery());
        restored.dispatch("scene.add", &CommandArgs::new(&[("kind", "cube")])).unwrap();
        assert!(restored.scene_snapshot().unwrap().0.starts_with(&root));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_scene_file_loads_the_backup() {
        let dir = std::env::temp_dir().join(format!("seal-scene-bak-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut session = EditorSession::new(Layout::default());
        session.user_scene = Some(dir.join("scene.seal"));
        session.save_scene().unwrap();
        let before = session.world.entities.len();
        session.dispatch("scene.add", &CommandArgs::new(&[("kind", "cube")])).unwrap();
        session.save_scene().unwrap();
        std::fs::remove_file(dir.join("scene.seal")).unwrap();
        let mut restored = EditorSession::new(Layout::default());
        restored.load_scene(&dir.join("scene.seal")).unwrap();
        assert_eq!(restored.world.entities.len(), before);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn named_layouts_round_trip_through_files() {
        let dir = std::env::temp_dir().join(format!("seal-layouts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut session = EditorSession::new(Layout::default());
        session.layouts_dir = Some(dir.clone());
        session.dispatch("layout.save", &CommandArgs::new(&[("name", "mine")])).unwrap();
        let mut restored = EditorSession::new(Layout::default());
        restored.layouts_dir = Some(dir.clone());
        restored.load_named_layouts().unwrap();
        restored.dispatch("layout.load", &CommandArgs::new(&[("name", "mine")])).unwrap();
        assert!(restored.layout_epoch >= 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn undo_history_stops_at_the_configured_limit() {
        let mut session = EditorSession::new(Layout::default());
        session.foundation.settings.set_user("undo.max_steps", "2").unwrap();
        session.dispatch("scene.add", &CommandArgs::new(&[("kind", "cube")])).unwrap();
        session.dispatch("scene.add", &CommandArgs::new(&[("kind", "sphere")])).unwrap();
        session.dispatch("scene.add", &CommandArgs::new(&[("kind", "plane")])).unwrap();
        assert_eq!(session.past.len(), 2);
    }

    #[test]
    fn autosave_waits_for_idle_unless_the_cap_is_reached() {
        assert!(!autosave_due(100, 100, 2000, 30_000));
        assert!(autosave_due(2000, 2000, 2000, 30_000));
        assert!(autosave_due(100, 30_000, 2000, 30_000));
        assert!(!autosave_due(400, 400, 100, 50));
    }

    #[test]
    fn a_verified_save_drops_the_scene_journal_and_can_clear_undo() {
        let dir = std::env::temp_dir().join(format!("seal-save-clear-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut session = EditorSession::new(Layout::default());
        session.user_scene = Some(dir.join("scene.seal"));
        session.scene_journal = Some(dir.join("scene.journal"));
        session.undo_journal = Some(dir.join("undo.journal"));
        session.foundation.settings.set_user("undo.max_steps", "50").unwrap();
        session.foundation.settings.set_user("undo.max_memory_mb", "0").unwrap();
        session.dispatch("scene.add", &CommandArgs::new(&[("kind", "cube")])).unwrap();
        session.dispatch("scene.add", &CommandArgs::new(&[("kind", "sphere")])).unwrap();
        assert_eq!(session.past.len(), 1);
        session.foundation.settings.set_user("undo.clear_on_save", "true").unwrap();
        session.save_scene().unwrap();
        assert!(session.past.is_empty());
        assert!(crate::journal_crc::read_records(&dir.join("scene.journal")).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn recovery_offers_restore_keep_both_and_discard_after_an_unclean_exit() {
        let root = std::env::temp_dir().join(format!("seal-recovery-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        let restore_dir = root.join("restore");
        let (mut session, saved) = reopen_crashed_scene(&restore_dir);
        assert!(session.world.entities.len() > saved);
        assert!(!session.past.is_empty());
        session.dispatch("recovery.restore", &CommandArgs::default()).unwrap();
        assert!(session.world.entities.len() > saved);
        assert!(!session.past.is_empty());
        assert!(session.recovered.iter().all(|doc| doc.id != "scene"));
        assert!(crate::journal_crc::read_records(&restore_dir.join("scene.journal"))
            .unwrap()
            .is_empty());
        let on_disk = seal_world::World::from_text(
            &std::fs::read_to_string(restore_dir.join("scene.seal")).unwrap(),
        )
        .unwrap();
        assert_eq!(on_disk.entities.len(), session.world.entities.len());

        let keep_dir = root.join("keep");
        let (mut session, saved) = reopen_crashed_scene(&keep_dir);
        session.dispatch("recovery.keep_both", &CommandArgs::default()).unwrap();
        assert!(session.world.entities.len() > saved);
        let kept = crate::journal_crc::read_records(&keep_dir.join("scene.journal.kept")).unwrap();
        assert!(!kept.is_empty());
        let on_disk = seal_world::World::from_text(
            &std::fs::read_to_string(keep_dir.join("scene.seal")).unwrap(),
        )
        .unwrap();
        assert!(on_disk.entities.len() > saved);

        let discard_dir = root.join("discard");
        let (mut session, saved) = reopen_crashed_scene(&discard_dir);
        session.dispatch("recovery.discard", &CommandArgs::default()).unwrap();
        assert_eq!(session.world.entities.len(), saved);
        assert!(!discard_dir.join("scene.journal").exists());
        assert!(!discard_dir.join("undo.journal").exists());
        let on_disk = seal_world::World::from_text(
            &std::fs::read_to_string(discard_dir.join("scene.seal")).unwrap(),
        )
        .unwrap();
        assert_eq!(on_disk.entities.len(), saved);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn recovery_lists_a_scratch_journal_that_has_steps() {
        let dir =
            std::env::temp_dir().join(format!("seal-recovery-scratch-{}", std::process::id()));
        let (mut restored, _) = reopen_crashed_scene(&dir);
        restored.journal_path = Some(dir.join("scratch.journal"));
        restored.set_scratch_text("kept");
        let unclean = crate::take_unclean_start(&dir);
        restored.present_recovery(true, unclean).unwrap();
        assert!(restored.recovered.iter().any(|doc| doc.id == "scene"));
        assert!(restored.recovered.iter().any(|doc| doc.id == "scratch"));
        let notice = restored.notice.clone().unwrap_or_default();
        assert!(notice.contains('2'), "{notice}");
        assert!(notice.contains("documents"), "{notice}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_scene_edit_stars_the_title_until_autosave() {
        let mut session = EditorSession::new(Layout::default());
        session.user_scene = Some(std::path::PathBuf::from("level_01.scene"));
        session.mark_scene();
        let dirty = session.dirty_marks(false);
        assert!(dirty.title.contains("level_01.scene*"), "{}", dirty.title);
        assert_eq!(dirty.status, crate::dirty::DirtyStatus::Unsaved);
        assert!(session.panel_dirty("viewport"));
        session.note_scene_saved(session.scene_edits).unwrap();
        let saved = session.dirty_marks(true);
        assert!(!saved.title.contains('*'));
        assert_eq!(saved.status, crate::dirty::DirtyStatus::Saved);
        assert!(!session.panel_dirty("viewport"));
        let elsewhere = std::path::PathBuf::from("other-folder/scene.seal");
        session.save_scene_elsewhere(elsewhere.clone());
        assert_eq!(session.scene_path().as_deref(), Some(elsewhere.as_path()));
        assert!(session.scene_dirty);
        session.set_scratch_text("half a note");
        assert!(session.panel_dirty("scratch"));
        assert!(session.dirty_marks(true).title.contains('*'));
    }

    #[test]
    fn a_toast_disappears_after_the_configured_timeout() {
        let mut session = EditorSession::new(Layout::default());
        session.foundation.settings.set_user("notifications.timeout_ms", "2000").unwrap();
        session.foundation.notices.push("flash", false, 0);
        session.foundation.notices.push("kept", true, 0);
        let timeout =
            session.foundation.settings.get("notifications.timeout_ms").unwrap().parse().unwrap();
        assert_eq!(session.foundation.notices.visible(1_500, timeout).len(), 2);
        let later = session.foundation.notices.visible(2_500, timeout);
        assert_eq!(later.len(), 1);
        assert!(later[0].error);
    }

    #[test]
    fn approval_blocks_the_agent_bus_until_it_is_turned_off() {
        let mut session = EditorSession::new(Layout::default());
        session.foundation.settings.set_user("agent.bus.enabled", "true").unwrap();
        assert!(session.bus_run("app.toggle_fullscreen", "").is_err());
        session.foundation.settings.set_user("agent.bus.require_approval", "false").unwrap();
        session.bus_run("app.toggle_fullscreen", "").unwrap();
        assert!(session.foundation.fullscreen);
    }

    #[test]
    fn verbose_logging_keeps_trace_and_says_it_is_slow() {
        let mut session = EditorSession::new(Layout::default());
        session.foundation.settings.set_user("log.verbose", "true").unwrap();
        assert_eq!(session.logging_ceiling(), seal_log::Level::Trace);
        let notice = session.logging_notice().unwrap();
        assert!(notice.contains("slower"), "{notice}");
        let mut logger = seal_log::Logger::new(seal_log::LogMode::Editor)
            .with_ceiling(session.logging_ceiling());
        logger.record(seal_log::Level::Trace, "shell", "visible");
        assert_eq!(logger.records().len(), 1);
    }

    #[test]
    fn a_half_typed_name_is_restored_and_marked() {
        let dir = std::env::temp_dir().join(format!("seal-draft-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut session = EditorSession::new(Layout::default());
        session.memory_path = Some(dir.join("memory.seal"));
        session
            .dispatch("draft.set", &CommandArgs::new(&[("key", "rename:crate"), ("value", "Cr")]))
            .unwrap();
        assert!(session.draft_is_marked("rename:crate", "Crate"));
        let mut restored = EditorSession::new(Layout::default());
        restored.memory_path = Some(dir.join("memory.seal"));
        restored.load_memory().unwrap();
        assert_eq!(restored.draft("rename:crate").as_deref(), Some("Cr"));
        assert!(restored.draft_is_marked("rename:crate", "Crate"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn opening_a_copied_project_folder_gives_the_copy_its_own_id() {
        let dir = std::env::temp_dir().join(format!("seal-open-copy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let first = dir.join("first");
        let made = crate::create_project(&first, "Game").unwrap();
        let copy = dir.join("copy");
        std::fs::create_dir_all(&copy).unwrap();
        std::fs::copy(first.join("project.seal"), copy.join("project.seal")).unwrap();
        let mut session = EditorSession::new(Layout::default());
        session
            .dispatch("file.open-project", &CommandArgs::new(&[("path", copy.to_str().unwrap())]))
            .unwrap();
        let opened = session.project.as_ref().unwrap().id.clone();
        assert_ne!(opened, made.id);
        assert_eq!(crate::open_project(&first).unwrap().id, made.id, "the original keeps its id");
        session.project_lock = None;
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn palette_and_notice_history_survive_restart() {
        let dir = std::env::temp_dir().join(format!("seal-history-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut session = EditorSession::new(Layout::default());
        session.memory_path = Some(dir.join("memory.seal"));
        session.dispatch("window.reset-layout", &CommandArgs::default()).unwrap();
        session.notify("kept notice", true);
        let mut restored = EditorSession::new(Layout::default());
        restored.memory_path = Some(dir.join("memory.seal"));
        restored.load_memory().unwrap();
        restored.palette_query.clear();
        let hits = restored.palette_hits();
        assert_eq!(hits.first().map(|hit| hit.id.as_str()), Some("window.reset-layout"));
        assert!(restored
            .foundation
            .notices
            .history()
            .iter()
            .any(|notice| notice.text == "kept notice"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn reopen_crashed_scene(dir: &std::path::Path) -> (EditorSession, usize) {
        let _ = std::fs::remove_dir_all(dir);
        std::fs::create_dir_all(dir).unwrap();
        let mut session = EditorSession::new(Layout::default());
        session.user_scene = Some(dir.join("scene.seal"));
        session.scene_journal = Some(dir.join("scene.journal"));
        session.undo_journal = Some(dir.join("undo.journal"));
        session.save_scene().unwrap();
        let saved = session.world.entities.len();
        session.dispatch("scene.add", &CommandArgs::new(&[("kind", "cube")])).unwrap();
        assert!(session.world.entities.len() > saved);
        let mut restored = EditorSession::new(Layout::default());
        restored.user_scene = Some(dir.join("scene.seal"));
        restored.scene_journal = Some(dir.join("scene.journal"));
        restored.undo_journal = Some(dir.join("undo.journal"));
        restored.load_scene(&dir.join("scene.seal")).unwrap();
        assert_eq!(restored.world.entities.len(), saved);
        assert!(restored.recover_journals().unwrap());
        let unclean = crate::take_unclean_start(dir);
        assert!(unclean);
        restored.present_recovery(true, unclean).unwrap();
        assert_eq!(restored.foundation.focused_panel_id, "recovery");
        assert!(restored.foundation.open_panels.iter().any(|id| id == "recovery"));
        assert_eq!(restored.reveal_panel.as_deref(), Some("recovery"));
        assert_eq!(restored.recovered.len(), 1);
        assert_eq!(restored.recovered[0].id, "scene");
        let notice = restored.notice.clone().unwrap_or_default();
        assert!(notice.contains("Recovery"), "{notice}");
        assert!(notice.contains('1'), "{notice}");
        assert!(notice.contains("document"), "{notice}");
        assert!(!notice.contains("documents"), "{notice}");
        (restored, saved)
    }

    #[test]
    fn unknown_panel_state_survives_save_and_load() {
        let dir = std::env::temp_dir().join(format!("seal-panel-future-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("session.seal");
        let mut session = EditorSession::new(Layout::default());
        session.models.panel_states.insert("panel.future".into(), "filter=lamp".into());
        session.save_panel_state(&path).unwrap();
        let mut again = EditorSession::new(Layout::default());
        again.load_panel_state(&path).unwrap();
        assert_eq!(
            again.models.panel_states.get("panel.future").map(String::as_str),
            Some("filter=lamp")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn panel_state_keeps_the_viewport_camera() {
        let dir = std::env::temp_dir().join(format!("seal-camera-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut session = EditorSession::new(Layout::default());
        session.set_draft("viewport-yaw", "15").unwrap();
        session.set_draft("viewport-pan-x", "2").unwrap();
        let path = dir.join("session.seal");
        session.save_panel_state(&path).unwrap();
        let mut loaded = EditorSession::new(Layout::default());
        loaded.load_panel_state(&path).unwrap();
        assert_eq!(loaded.draft("viewport-yaw").as_deref(), Some("15"));
        assert_eq!(loaded.draft("viewport-pan-x").as_deref(), Some("2"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod transient_draft_tests {
    use super::*;

    #[test]
    fn an_armed_brush_is_not_restored_but_typed_drafts_are() {
        let dir = std::env::temp_dir().join(format!("seal-brush-draft-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut first = EditorSession::new(Layout::default());
        first.memory_path = Some(dir.join("memory.seal"));
        first.set_draft("place-brush", "sprites/grass.png").unwrap();
        first.set_draft("rename", "Coin 4").unwrap();
        let mut second = EditorSession::new(Layout::default());
        second.memory_path = Some(dir.join("memory.seal"));
        second.load_memory().unwrap();
        assert_eq!(second.draft("place-brush"), None);
        assert_eq!(second.draft("rename").as_deref(), Some("Coin 4"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod delete_survives_tests {
    use super::*;

    #[test]
    fn select_all_follows_the_outliner_search_and_skips_the_root() {
        let mut session = EditorSession::new(Layout::default());
        session.world.entities.retain(|e| e.id == "root");
        session.adopt_world();
        for kind in ["cube", "cube", "sphere"] {
            session.dispatch("scene.add", &CommandArgs::new(&[("kind", kind)])).unwrap();
        }
        let added = session.world.entities.iter().rev().next().unwrap().id.clone();
        assert_eq!(session.foundation.selection, vec![added], "a new object is selected alone");
        session.dispatch("edit.select_all", &CommandArgs::default()).unwrap();
        assert_eq!(session.foundation.selection.len(), 3);
        assert!(!session.foundation.selection.iter().any(|id| id == "root"));
        session.models.outliner_query = "cube".into();
        session.dispatch("edit.select_all", &CommandArgs::default()).unwrap();
        assert_eq!(session.foundation.selection.len(), 2, "only the rows the search shows");
    }

    #[test]
    fn opening_the_manifest_file_opens_its_project_and_closing_saves() {
        let dir = std::env::temp_dir().join(format!("seal-manifest-open-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let folder = dir.join("game");
        let mut session = EditorSession::new(Layout::default());
        session.recent_path = Some(dir.join("recent.seal"));
        let path = folder.to_str().unwrap().to_string();
        session
            .dispatch("file.new-project", &CommandArgs::new(&[("path", &path), ("name", "Game")]))
            .unwrap();
        session.dispatch("scene.add", &CommandArgs::new(&[("kind", "cube")])).unwrap();
        session.dispatch("file.close-project", &CommandArgs::default()).unwrap();
        assert!(session.project.is_none());
        let manifest = folder.join("project.seal");
        session
            .dispatch(
                "file.open-project",
                &CommandArgs::new(&[("path", manifest.to_str().unwrap())]),
            )
            .unwrap();
        assert!(session.project.is_some());
        assert!(
            session.world.entities.iter().any(|e| e.name.starts_with("Cube")),
            "close saved it"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_deleted_tile_stays_gone_after_undo_of_later_work_and_reopen() {
        let dir = std::env::temp_dir().join(format!("seal-del-undo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let folder = dir.join("game");
        let mut session = EditorSession::new(Layout::default());
        session.recent_path = Some(dir.join("recent.seal"));
        let path = folder.to_str().unwrap().to_string();
        session
            .dispatch("file.new-project", &CommandArgs::new(&[("path", &path), ("name", "Game")]))
            .unwrap();
        for x in ["0", "1", "2"] {
            session
                .dispatch("scene.add", &CommandArgs::new(&[("kind", "cube"), ("a", x), ("b", "0")]))
                .unwrap();
        }
        let doomed = session.world.entities.iter().find(|e| e.name == "Cube 2").unwrap().id.clone();
        session.dispatch("outliner.select", &CommandArgs::new(&[("id", &doomed)])).unwrap();
        session.dispatch("edit.delete", &CommandArgs::default()).unwrap();
        assert!(!session.world.entities.iter().any(|e| e.id == doomed));
        session
            .dispatch("scene.add", &CommandArgs::new(&[("kind", "cube"), ("a", "5"), ("b", "0")]))
            .unwrap();
        session.dispatch("edit.undo", &CommandArgs::default()).unwrap();
        assert!(!session.world.entities.iter().any(|e| e.name == "Cube 2"), "undo kept the delete");
        session.flush_scene_journals().unwrap();
        session.dispatch("file.open-project", &CommandArgs::new(&[("path", &path)])).unwrap();
        let names: Vec<_> = session.world.entities.iter().map(|e| e.name.clone()).collect();
        assert!(!names.iter().any(|n| n == "Cube 2"), "reopen kept the delete: {names:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn picking_a_tool_or_opening_a_project_stops_the_sprite_brush() {
        let dir = std::env::temp_dir().join(format!("seal-brush-stop-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let folder = dir.join("game");
        let path = folder.to_str().unwrap().to_string();
        let mut session = EditorSession::new(Layout::default());
        session.recent_path = Some(dir.join("recent.seal"));
        session
            .dispatch("file.new-project", &CommandArgs::new(&[("path", &path), ("name", "Game")]))
            .unwrap();
        session.set_draft("place-brush", "sprites/desk.png").unwrap();
        session.dispatch("viewport.tool", &CommandArgs::new(&[("tool", "select")])).unwrap();
        assert_eq!(session.draft("place-brush").as_deref(), Some(""));
        session.set_draft("place-brush", "sprites/desk.png").unwrap();
        session.dispatch("file.open-project", &CommandArgs::new(&[("path", &path)])).unwrap();
        assert!(session.draft("place-brush").is_none_or(|b| b.is_empty()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
