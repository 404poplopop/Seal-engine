//! A scene the editor can edit and the play loop can step.
//!
//! Play builds a rigid-body simulation from the scene. The scene file does not
//! store solver state, so stop can put the entities back exactly.
#![forbid(unsafe_code)]

mod ecs;
mod platformer;
mod prop_apply;
mod prop_fields;
#[cfg(feature = "play-preview")]
mod sim;
#[cfg(feature = "play-preview")]
pub use sim::CharacterRules;

pub use ecs::{engine_world, Body, Camera, EditorInfo, Light, Look};
pub use platformer::PlayInput;
pub use prop_apply::ui_node_value;
pub use prop_fields::entity_field_paths;
pub mod controls;
pub mod curve;
pub mod curve_text;
pub mod data_asset;
pub mod prefab_variant;
pub mod sound_container;
pub use seal_input;
pub use seal_scene;
pub mod ai;
pub mod anim_events;
pub mod area_effect;
pub mod audio_listener;
pub mod cam_drag;
pub mod camera2d;
pub mod character2d;
pub mod cloud;
pub mod collider_edit;
pub mod collider_gate;
pub mod dialog_ui;
pub mod extra_window;
pub mod file_dialog;
pub mod fonts;
pub mod frames;
pub mod game;
pub mod gameui;
pub mod light_fields;
pub mod material2d;
pub mod mesh_surface;
pub mod nav;
pub mod nav2d;
pub mod net;
pub mod object_files;
pub mod object_merge;
pub mod particles;
pub mod phys2d;
pub mod physics;
pub mod post_volume;
pub mod prefab_nest;
pub mod query_shapes;
pub mod remote_transform;
pub mod savegame;
pub mod scenes;
pub mod script_groups;
pub mod scripts;
pub mod sprite_socket;
pub mod text_encoding;
pub mod text_layout;
pub mod text_sprites;
pub mod tile_merge;

pub use audio_listener::{active_listener, ActiveListener};
pub mod players;
pub mod shape2d;
pub mod shape_style;
pub mod sheet;
pub mod sprite_mask;
pub mod sprite_pivot;
pub mod sprite_shape;
pub mod sprite_slice;
pub mod sprite_sort;
pub mod volumes;

/// Play steps a rigid-body preview only when the crate is built with `play-preview`.
pub fn play_preview_enabled() -> bool {
    cfg!(feature = "play-preview")
}

use seal_sealtext::{parse, SealValue};

/// Whether the world is being edited, simulated, or held.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayMode {
    Editing,
    Playing,
    Paused,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Cube,
    Sphere,
    Cylinder,
    Cone,
    Capsule,
    Plane,
}

impl Shape {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "cube" => Some(Self::Cube),
            "sphere" => Some(Self::Sphere),
            "cylinder" => Some(Self::Cylinder),
            "cone" => Some(Self::Cone),
            "capsule" => Some(Self::Capsule),
            "plane" => Some(Self::Plane),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Cube => "cube",
            Self::Sphere => "sphere",
            Self::Cylinder => "cylinder",
            Self::Cone => "cone",
            Self::Capsule => "capsule",
            Self::Plane => "plane",
        }
    }
}

pub const DEFAULT_GRAVITY: f32 = 9.81;

/// One object in the scene. `tag` is free text the inspector can edit.
#[derive(Clone, Debug, PartialEq)]
pub struct Entity {
    pub id: String,
    pub name: String,
    pub parent: Option<String>,
    pub translation: [f32; 3],
    pub shape: Option<Shape>,
    pub dynamic: bool,
    /// Surface grip in the play physics.
    pub friction: f32,
    /// Bounce in the play physics.
    pub restitution: f32,
    /// Walking speed of a character the player drives in play; 0 = not a character.
    pub walk_speed: f32,
    pub velocity: [f32; 3],
    pub tag: String,
    /// Degrees around x, y, then z.
    pub rotation: [f32; 3],
    pub scale: [f32; 3],
    /// Red, green, and blue, each from 0 to 1.
    pub color: [f32; 3],
    pub hidden: bool,
    pub locked: bool,
    /// When true, the entity is not picked in the viewport or multi-select paths.
    pub unselectable: bool,
    pub roughness: f32,
    pub metal: f32,
    pub intensity: f32,
    pub fov: f32,
    pub near: f32,
    pub far: f32,
    /// `prefab` or `script` when this row is an asset instance.
    pub source_kind: String,
    /// Asset name shown as `Edit <asset>` in the outliner type column.
    pub source_asset: String,
    /// Prefab copies: comma-separated fields changed on this copy; a rebuild keeps them.
    pub overrides: String,
    /// Row shows `*` when true.
    pub outliner_unsaved: bool,
    /// Row label is italic when spawned during play.
    pub outliner_play_spawned: bool,
    /// Fluent id for the warning tooltip; yellow label when set.
    pub outliner_warning_key: String,
    /// Picture drawn instead of a shape, relative to the project folder; empty for none.
    pub sprite: String,
    /// Draw order for sprites: higher layers draw on top; equal layers fall back to depth.
    pub layer: i32,
    /// Sprite sheet frames laid out left to right; 1 is a still picture.
    pub frames: u32,
    /// Frames per second while playing.
    pub fps: f32,
    /// Sheet frame forced by a sprite_frame animation track while it runs; None plays the sheet by time.
    pub frame_override: Option<u32>,
    /// Clip played when `sprite` is a `.frames` animation file; empty plays the first clip.
    pub animation: String,
    /// Play time when the current clip started (not saved).
    pub anim_start: f32,
    /// Mirror the picture left-right and top-bottom.
    pub flip: [bool; 2],
    /// Colour multiplied into the sprite: red, green, blue and opacity, each 0 to 1.
    pub tint: [f32; 4],
    /// Draw order among sprites on the same layer: higher draws on top.
    pub order: i32,
    /// Nine-slice borders in picture pixels: left, right, top, bottom; all zero draws whole.
    pub slice: [f32; 4],
    /// Nine-slice edges: `stretch` (default) or `tile`.
    pub slice_mode: String,
    /// World width and height for a sliced sprite; zero keeps scale as today.
    pub slice_size: [f32; 2],
    /// Pivot inside the picture: 0,0 bottom left, 0.5,0.5 centre.
    pub pivot: [f32; 2],
    /// Draw order rule for this sprite; `default` uses the sorting layer table.
    pub sort_mode: sprite_sort::ObjectSortMode,
    /// Normal map picture for 2D lights, relative to the project folder; empty for none.
    pub normal_map: String,
    /// Parallax scroll factor across and up: 1 moves with the world, 0 stays on screen.
    pub parallax: [f32; 2],
    /// Repeat the picture side by side across the view.
    pub repeat_x: bool,
    /// 2D light settings; an empty kind means this is not a 2D light.
    pub light2d: seal_scene::Light2d,
    /// 2D play camera settings; off unless `camera2d.on`.
    pub camera2d: seal_scene::Camera2d,
    /// Positional audio origin while this listener is enabled.
    pub audio_listener: seal_scene::AudioListener,
    /// 2D physics body, shape and joint; no body unless `phys2d.body` is set.
    pub phys2d: seal_scene::Physics2d,
    /// Area effect on a trigger collider. Empty mode means none.
    pub area: seal_physics2d::AreaEffect,
    /// Moving walkway speed along the collider. 0 means none.
    pub surface: seal_physics2d::SurfaceEffect,
    /// 2D walker settings; off unless `nav2d.on`.
    pub nav2d: seal_scene::NavAgent2d,
    /// Kinematic 2D character; off unless `character2d.on`.
    pub character2d: seal_scene::Character2d,
    /// A drawn 2D line or filled shape.
    pub shape2d: seal_scene::Shape2d,
    /// Spline terrain: a fill, an edge strip and an optional collider.
    pub sprite_shape: seal_scene::SpriteShape,
    /// Following another object's line during play.
    pub follow: seal_scene::PathFollow,
    pub material2d: seal_scene::Material2d,
    /// How this sprite is clipped by sprite masks: empty or `none`, `inside`, `outside`.
    pub mask: String,
    /// Sprite mask on this object. An empty shape means it is not a mask.
    pub sprite_mask: seal_scene::SpriteMask,
    /// Logic graph run on this object during play, relative to the project folder; empty for none.
    pub script: String,
    /// Particle effect (`.fx`) run on this object during play, relative to the project folder; empty for none.
    pub particles: String,
    /// Socket on the parent sprite animation this object follows. Empty means it stays put.
    pub socket: String,
    /// Kill, damage, checkpoint, trigger, or post-process volume. Empty kind means none.
    pub volume: volumes::Volume,
    /// Lower graphs run first. Zero keeps the scene's own order.
    pub script_order: i32,
    /// `after_physics` or `late`. Empty runs before physics, with the ordinary tick.
    pub script_group: String,
    /// Copies this object's transform onto another during play. Off until `on`.
    pub remote: remote_transform::RemoteTransform,
    /// When false, the 2D collider does not block and queries skip it.
    pub collider_enabled: bool,
    /// Sprites or triangles kept after Merge Objects. Empty kind means this is not a merge.
    pub merged: object_merge::MergedDraw,
    /// Post-process settings. Used when `volume.kind` is `post`.
    pub post: post_volume::PostVolume,
    /// Base colour texture for a mesh. Empty keeps the flat colour.
    pub mesh_material: seal_scene::MeshMaterial,
}

impl Entity {
    /// Which sheet frame shows `seconds` into play (0 while editing or for a still picture).
    pub fn sprite_frame(&self, seconds: f32) -> u32 {
        if let Some(f) = self.frame_override {
            return f.min(self.frames.max(1) - 1);
        }
        if self.frames <= 1 || self.fps <= 0.0 || seconds <= 0.0 {
            return 0;
        }
        ((seconds * self.fps).floor() as u64 % self.frames as u64) as u32
    }

    /// Left and right texture coordinates of the frame showing `seconds` into play.
    pub fn sprite_uv(&self, seconds: f32) -> [f32; 2] {
        let n = self.frames.max(1) as f32;
        let i = self.sprite_frame(seconds) as f32;
        [i / n, (i + 1.0) / n]
    }

    /// Picture file and texture rectangle (u0, v0, u1, v1, v down) to draw
    /// `seconds` into play, with flips applied. `project` resolves relative
    /// paths. A `.frames` sprite plays its current clip from `anim_start`.
    pub fn sprite_draw(
        &self,
        project: Option<&std::path::Path>,
        seconds: f32,
    ) -> Option<(std::path::PathBuf, [f32; 4])> {
        if self.sprite.is_empty() {
            return None;
        }
        let raw = std::path::Path::new(&self.sprite);
        let full = match project {
            Some(root) if raw.is_relative() => root.join(raw),
            _ => raw.to_path_buf(),
        };
        let (image, mut uv) = if frames::is_frames_path(&self.sprite) {
            let lib = frames::load_cached(&full)?;
            let clip = lib.clip_or_first(&self.animation)?;
            let idx = match self.frame_override {
                Some(f) => (f as usize).min(clip.frames.len().saturating_sub(1)),
                None => clip.frame_at(if seconds > 0.0 { seconds - self.anim_start } else { 0.0 }),
            };
            let frame = clip.frames.get(idx)?;
            let dir = full.parent().map(|p| p.to_path_buf()).unwrap_or_default();
            (dir.join(&frame.image), frame.uv)
        } else {
            let [u0, u1] = self.sprite_uv(seconds);
            (full, [u0, 0.0, u1, 1.0])
        };
        if self.flip[0] {
            uv.swap(0, 2);
        }
        if self.flip[1] {
            uv.swap(1, 3);
        }
        Some((image, uv))
    }

    /// Start clip `name` of a `.frames` sprite from its first frame; the
    /// same clip already playing keeps going.
    pub fn play_animation(&mut self, name: &str, now: f32) {
        if self.animation != name {
            self.animation = name.into();
            self.anim_start = now;
        }
    }

    pub fn inspector_fields(&self) -> Vec<(String, String)> {
        let mut fields = Vec::new();
        fields.push(("role".into(), self.tag.clone()));
        fields.push(("x".into(), format_num(self.translation[0])));
        fields.push(("y".into(), format_num(self.translation[1])));
        fields.push(("z".into(), format_num(self.translation[2])));
        fields.push(("rx".into(), format_num(self.rotation[0])));
        fields.push(("ry".into(), format_num(self.rotation[1])));
        fields.push(("rz".into(), format_num(self.rotation[2])));
        fields.push(("sx".into(), format_num(self.scale[0])));
        fields.push(("sy".into(), format_num(self.scale[1])));
        fields.push(("sz".into(), format_num(self.scale[2])));
        fields.push(("r".into(), format_num(self.color[0])));
        fields.push(("g".into(), format_num(self.color[1])));
        fields.push(("b".into(), format_num(self.color[2])));
        fields.push(("roughness".into(), format_num(self.roughness)));
        fields.push(("metal".into(), format_num(self.metal)));
        fields.push(("intensity".into(), format_num(self.intensity)));
        fields.push(("fov".into(), format_num(self.fov)));
        fields.push(("near".into(), format_num(self.near)));
        fields.push(("far".into(), format_num(self.far)));
        fields.push(("hidden".into(), if self.hidden { "yes" } else { "no" }.into()));
        fields.push(("locked".into(), if self.locked { "yes" } else { "no" }.into()));
        fields.push(("unselectable".into(), if self.unselectable { "yes" } else { "no" }.into()));
        fields.push(("shape".into(), self.shape.map(Shape::label).unwrap_or("none").into()));
        fields.push(("motion".into(), if self.dynamic { "dynamic" } else { "static" }.into()));
        fields.push(("friction".into(), format_num(self.friction)));
        fields.push(("bounce".into(), format_num(self.restitution)));
        fields.push(("walk_speed".into(), format_num(self.walk_speed)));
        fields.push(("source_kind".into(), self.source_kind.clone()));
        fields.push(("source_asset".into(), self.source_asset.clone()));
        fields.push(("overrides".into(), self.overrides.clone()));
        fields.push(("unsaved".into(), if self.outliner_unsaved { "yes" } else { "no" }.into()));
        fields.push((
            "spawned_play".into(),
            if self.outliner_play_spawned { "yes" } else { "no" }.into(),
        ));
        if !self.sprite.is_empty() {
            fields.push(("sprite".into(), self.sprite.clone()));
            fields.push(("layer".into(), self.layer.to_string()));
            fields.push(("frames".into(), self.frames.max(1).to_string()));
            fields.push(("fps".into(), format_num(self.fps)));
            fields.push(("animation".into(), self.animation.clone()));
            fields.push(("flip_x".into(), if self.flip[0] { "yes" } else { "no" }.into()));
            fields.push(("flip_y".into(), if self.flip[1] { "yes" } else { "no" }.into()));
            fields
                .push(("tint".into(), tint_hex(&[self.tint[0], self.tint[1], self.tint[2], 1.0])));
            fields.push(("opacity".into(), format_num(self.tint[3])));
            fields.push(("order".into(), self.order.to_string()));
            fields.push(("slice".into(), slice_text(&self.slice)));
            if self.slice_mode != "stretch" {
                fields.push(("slice_mode".into(), self.slice_mode.clone()));
            }
            if self.slice_size != [0.0; 2] {
                fields.push(("slice_size".into(), sprite_slice::slice_size_text(self.slice_size)));
            }
            if self.pivot != [0.5, 0.5] {
                fields.push(("pivot".into(), sprite_pivot::pivot_text(self.pivot)));
            }
            if self.sort_mode != sprite_sort::ObjectSortMode::Default {
                fields.push(("sort_mode".into(), self.sort_mode.name().into()));
            }
            fields.push(("normal_map".into(), self.normal_map.clone()));
            fields.push(("parallax".into(), parallax_text(&self.parallax)));
            fields.push(("repeat_x".into(), if self.repeat_x { "yes" } else { "no" }.into()));
            if let Some(row) = sprite_mask::interaction_field(self) {
                fields.push(row);
            }
        }
        fields.extend(sprite_mask::fields(self));
        if !self.light2d.kind.is_empty() {
            fields.push(("light2d".into(), self.light2d.kind.clone()));
            fields.push(("light_range".into(), format_num(self.light2d.range)));
            fields.push(("light_cone".into(), format_num(self.light2d.cone)));
            fields.push((
                "light_shadows".into(),
                if self.light2d.shadows { "yes" } else { "no" }.into(),
            ));
            fields.extend(light_fields::rows(self));
        }
        if self.camera2d.on || self.tag == "camera" {
            let c = &self.camera2d;
            fields.push(("camera2d".into(), if c.on { "yes" } else { "no" }.into()));
            fields.push(("cam_follow".into(), c.follow.clone()));
            fields.push(("cam_smoothing".into(), format_num(c.smoothing)));
            fields.push(("cam_zoom".into(), format_num(c.zoom)));
            fields.push(("cam_limits".into(), slice_text(&c.limits)));
            fields.push(("cam_pixels".into(), format_num(c.pixel_snap)));
            fields.extend(cam_drag::rows(c));
            fields.push(("cam_cull_mask".into(), phys2d::layers_text(c.cull_mask)));
        }
        if self.audio_listener.on || self.tag == "camera" || self.camera2d.on {
            fields.push((
                "audio_listener".into(),
                if self.audio_listener.on { "yes" } else { "no" }.into(),
            ));
            fields.push((
                "listener_max_distance".into(),
                format_num(self.audio_listener.max_distance),
            ));
            fields.push(("listener_rolloff".into(), format_num(self.audio_listener.rolloff)));
        }
        if !matches!(self.tag.as_str(), "camera" | "light") {
            fields.extend(phys2d::fields(&self.phys2d));
            fields.extend(area_effect::fields(&self.area, &self.surface));
            fields.extend(nav2d::fields(&self.nav2d));
            fields.extend(character2d::fields(&self.character2d));
            fields.extend(shape2d::shape_fields(self));
            fields.extend(shape2d::follow_fields(self));
            fields.extend(sprite_shape::shape_fields(self));
            fields.extend(material2d::fields(self));
        }
        if !self.script.is_empty() {
            fields.push(("script".into(), self.script.clone()));
            fields.push(("script_order".into(), self.script_order.to_string()));
            fields.push(("script_group".into(), script_groups::name(&self.script_group).into()));
        } else if self.script_order != 0
            || script_groups::name(&self.script_group) != script_groups::BEFORE
        {
            if self.script_order != 0 {
                fields.push(("script_order".into(), self.script_order.to_string()));
            }
            fields.push(("script_group".into(), script_groups::name(&self.script_group).into()));
        }
        fields.extend(remote_transform::fields(&self.remote));
        fields.extend(volumes::fields(&self.volume));
        if self.volume.kind == "post" {
            fields.extend(post_volume::fields(&self.post));
        }
        if !self.particles.is_empty() {
            fields.push(("particles".into(), self.particles.clone()));
        }
        if !self.socket.is_empty() {
            fields.push(("socket".into(), self.socket.clone()));
        }
        if !self.outliner_warning_key.is_empty() {
            fields.push(("outliner_warning".into(), self.outliner_warning_key.clone()));
        }
        if self.shape.is_some() || !self.mesh_material.texture.is_empty() {
            fields.extend(mesh_surface::rows(self));
        }
        fields
    }
}

/// The editable scene, plus a snapshot taken when play begins.
pub struct World {
    pub entities: Vec<Entity>,
    pub mode: PlayMode,
    /// Downward pull along y while play runs, in units per second squared.
    pub gravity: f32,
    /// Multiplier on the play step. Zero pauses motion without leaving play.
    pub time_scale: f32,
    /// 2D physics settings copied from the project when play starts.
    pub phys2d_settings: seal_physics2d::Physics2DSettings,
    snapshot: Option<Vec<Entity>>,
    #[cfg(feature = "play-preview")]
    sim: Option<sim::Sim>,
    /// Side-view gameplay, when the scene has a player with a picture.
    platformer: Option<platformer::Platformer>,
    /// 2D bodies in a scene without a platformer player.
    sim2d: Option<phys2d::Sim2d>,
    /// Buttons held this frame, read by side-view gameplay.
    pub play_input: PlayInput,
    /// Folder that object scripts are found in; scripts do not run without it.
    pub project_dir: Option<std::path::PathBuf>,
    /// Keys pressed (`true`) and released since the last step, by graph key
    /// name; object scripts receive them at the start of the next step.
    pub play_keys: Vec<(String, bool)>,
    /// In-game UI screens shown while playing.
    pub game_screens: gameui::Screens,
    /// Sounds object scripts started during play, for the editor to play.
    pub play_sounds: Vec<scripts::SoundCue>,
    /// Music-bus commands for the editor to play. Scene music is queued when play begins.
    pub play_music: Vec<scripts::MusicCue>,
    /// Project-relative sound file started on the music bus when play begins. Empty plays nothing.
    pub music: String,
    /// Cubemap `.texture` drawn behind the perspective view and play. Empty draws no sky image.
    pub sky_cubemap: String,
    /// Soft glow for the flat 2D view. Off unless enabled, so older scenes stay as they were.
    pub glow: seal_scene::Glow2d,
    /// Breakpoint node ids by graph file; kept across plays.
    pub breakpoints: std::collections::BTreeMap<String, std::collections::BTreeSet<u32>>,
    /// Messages from object scripts during the current play.
    pub play_log: Vec<String>,
    /// Seconds of play so far (scaled by time scale); drives sprite sheet frames.
    pub play_time: f32,
    /// Raw play clock: ignores pause and time scale.
    pub unscaled_time: f64,
    /// Last raw frame length in seconds.
    pub unscaled_dt: f32,
    /// Play frames stepped since play began (wraps at u64::MAX).
    pub sim_tick: u64,
    scripts: Option<scripts::Scripts>,
    /// Running keyframe animations: object id, animation, play_time when started.
    pub anims: Vec<(String, seal_scene::anim::Anim, f32)>,
    /// Running sequences: sequence, play_time when started, last sequence time handled.
    pub sequences: Vec<(seal_scene::sequence::Sequence, f32, f32)>,
    /// Property paths already reported as missing during this play, so each warns once.
    sequence_warned: std::collections::BTreeSet<String>,
    /// Particle emitters running in play, each attached to an object id (it follows that object).
    pub emitters: Vec<(String, particles::Emitter)>,
    /// Network play: hosting for joined players, or showing a host's game. Ends on stop.
    pub net: Option<net::NetRole>,
    /// Names of sequence events fired during play, oldest first (also written to play_log).
    pub sequence_events: Vec<String>,
    /// Objects walking AI orders during play.
    pub ai: ai::Ai,
    /// 2D walkers during play.
    pub walkers: nav2d::Walkers,
    /// Kinematic 2D characters during play.
    pub characters: character2d::Motion,
    /// Objects riding along lines during play.
    pub followers: shape2d::Followers,
    /// The 2D play camera's moving state and any running shakes.
    pub camera_rig: camera2d::Rig,
    /// Half the play view, in scene units. Drag margins are fractions of it.
    camera_view_half: [f32; 2],
    /// The view looks through the play camera, so walking follows its facing.
    look_camera: bool,
    /// Last sampled sprite-animation time, so a frame event fires once per pass.
    anim_cursor: std::collections::BTreeMap<String, anim_events::Cursor>,
    /// Input bindings, pad rumble and rebinding while playing.
    pub controls: controls::Controls,
    /// The open scene, scenes added on top and objects kept across scenes.
    pub scenes: scenes::Scenes,
    /// A script set the game state to `quit`. Stays set so a packaged game can leave.
    pub quit: bool,
    /// Press Start to Join is saved with the scene.
    pub join_on: bool,
    /// Who is inside a volume, and leftover fractional damage, while playing.
    volume_memory: volumes::Memory,
    /// Gamepads given to players while playing.
    join_roster: players::Roster,
    /// Scores and checkpoints while playing a scene that has no scripts.
    rules: game::Game,
    /// When true, the scene is stored as one file per top-level object. Off leaves one scene file.
    pub objects_per_file: bool,
    /// Extra physics worlds, the handle server, and 3D collider shapes.
    pub physics: physics::PhysicsState,
}

/// How side-view gameplay is going, for the play overlay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayStatus {
    pub coins: usize,
    pub coins_total: usize,
    pub falls: u32,
    pub won: bool,
    /// The player just touched the flag with coins still missing.
    pub goal_locked: bool,
}

impl World {
    /// Scene for a brand-new project: the demo's folders, floor, lights and player start,
    /// without the sample shapes.
    pub fn starter() -> Self {
        let mut world = Self::demo();
        let doomed: Vec<String> = world
            .entities
            .iter()
            .filter(|e| e.parent.as_deref() == Some("geo") && e.id != "floor")
            .map(|e| e.id.clone())
            .collect();
        for id in doomed {
            world.remove(&id);
        }
        world
    }

    /// Starter scene for the default layout: folders, floor, shapes, lights, player start.
    pub fn demo() -> Self {
        Self {
            mode: PlayMode::Editing,
            gravity: DEFAULT_GRAVITY,
            time_scale: 1.0,
            phys2d_settings: seal_physics2d::Physics2DSettings::default(),
            snapshot: None,
            #[cfg(feature = "play-preview")]
            sim: None,
            platformer: None,
            sim2d: None,
            project_dir: None,
            play_keys: Vec::new(),
            game_screens: gameui::Screens::default(),
            play_sounds: Vec::new(),
            play_music: Vec::new(),
            music: String::new(),
            sky_cubemap: String::new(),
            glow: seal_scene::Glow2d::default(),
            breakpoints: Default::default(),
            play_log: Vec::new(),
            play_time: 0.0,
            unscaled_time: 0.0,
            unscaled_dt: 0.0,
            sim_tick: 0,
            anims: Vec::new(),
            sequences: Vec::new(),
            sequence_warned: std::collections::BTreeSet::new(),
            emitters: Vec::new(),
            net: None,
            sequence_events: Vec::new(),
            ai: ai::Ai::default(),
            walkers: nav2d::Walkers::default(),
            characters: character2d::Motion::default(),
            followers: shape2d::Followers::default(),
            camera_rig: camera2d::Rig::default(),
            camera_view_half: [0.0, 0.0],
            look_camera: false,
            anim_cursor: std::collections::BTreeMap::new(),
            controls: controls::Controls::default(),
            scenes: scenes::Scenes::default(),
            quit: false,
            join_on: false,
            volume_memory: volumes::Memory::default(),
            join_roster: players::Roster::default(),
            rules: game::Game::default(),
            objects_per_file: false,
            physics: physics::PhysicsState::default(),
            scripts: None,
            play_input: PlayInput::default(),
            entities: {
                let mut entities = vec![
                    entity("root", "World", None, [0.0, 0.0, 0.0], None, false, ""),
                    entity("geo", "Geometry", Some("root"), [0.0, 0.0, 0.0], None, false, "folder"),
                    entity(
                        "lighting",
                        "Lighting",
                        Some("root"),
                        [0.0, 0.0, 0.0],
                        None,
                        false,
                        "folder",
                    ),
                    entity(
                        "gameplay",
                        "Gameplay",
                        Some("root"),
                        [0.0, 0.0, 0.0],
                        None,
                        false,
                        "folder",
                    ),
                    entity(
                        "floor",
                        "Floor",
                        Some("geo"),
                        [0.0, 0.0, 0.0],
                        Some(Shape::Plane),
                        false,
                        "ground",
                    ),
                    entity(
                        "crate",
                        "Crate",
                        Some("geo"),
                        [1.5, 0.5, 0.0],
                        Some(Shape::Cube),
                        true,
                        "prop",
                    ),
                    entity(
                        "shape_red",
                        "Red Block",
                        Some("geo"),
                        [-1.5, 0.5, 0.5],
                        Some(Shape::Cube),
                        false,
                        "prop",
                    ),
                    entity(
                        "shape_teal",
                        "Teal Block",
                        Some("geo"),
                        [0.0, 0.5, 1.2],
                        Some(Shape::Cube),
                        false,
                        "prop",
                    ),
                    entity(
                        "shape_green",
                        "Green Block",
                        Some("geo"),
                        [0.8, 0.5, -1.0],
                        Some(Shape::Cube),
                        false,
                        "prop",
                    ),
                    entity(
                        "shape_blue",
                        "Blue Sphere",
                        Some("geo"),
                        [-0.8, 0.5, -0.5],
                        Some(Shape::Sphere),
                        false,
                        "prop",
                    ),
                    entity(
                        "shape_gold",
                        "Gold Sphere",
                        Some("geo"),
                        [2.0, 0.5, 1.0],
                        Some(Shape::Sphere),
                        false,
                        "prop",
                    ),
                    entity(
                        "shape_violet",
                        "Violet Cylinder",
                        Some("geo"),
                        [-2.6, 0.5, 1.4],
                        Some(Shape::Cylinder),
                        false,
                        "prop",
                    ),
                    entity("sun", "Sun", Some("lighting"), [-4.0, 7.0, 3.0], None, false, "light"),
                    entity(
                        "sky",
                        "Sky Light",
                        Some("lighting"),
                        [0.0, 6.0, 0.0],
                        None,
                        false,
                        "light",
                    ),
                    entity(
                        "hero",
                        "Player Start",
                        Some("gameplay"),
                        [0.0, 0.0, 0.0],
                        None,
                        false,
                        "player",
                    ),
                ];
                tint_entity(&mut entities, "shape_red", [0.92, 0.22, 0.2]);
                tint_entity(&mut entities, "shape_teal", [0.18, 0.78, 0.82]);
                tint_entity(&mut entities, "shape_green", [0.28, 0.82, 0.38]);
                tint_entity(&mut entities, "shape_blue", [0.28, 0.46, 0.95]);
                tint_entity(&mut entities, "shape_gold", [0.92, 0.78, 0.22]);
                tint_entity(&mut entities, "shape_violet", [0.62, 0.42, 0.95]);
                if let Some(hero) = entities.iter_mut().find(|e| e.id == "hero") {
                    hero.source_kind = "prefab".into();
                    hero.source_asset = "HeroPrefab".into();
                }
                if let Some(crate_ent) = entities.iter_mut().find(|e| e.id == "crate") {
                    crate_ent.outliner_unsaved = true;
                }
                if let Some(sun) = entities.iter_mut().find(|e| e.id == "sun") {
                    sun.outliner_warning_key = "outliner-warning-motion-mode".into();
                    sun.intensity = 6.0;
                }
                if let Some(sky) = entities.iter_mut().find(|e| e.id == "sky") {
                    sky.intensity = 1.8;
                }
                entities
            },
        }
    }

    pub fn entity(&self, id: &str) -> Option<&Entity> {
        self.entities.iter().find(|entity| entity.id == id)
    }

    pub fn entity_mut(&mut self, id: &str) -> Option<&mut Entity> {
        self.entities.iter_mut().find(|entity| entity.id == id)
    }

    /// Ask a Character 2D to move on the next play step. `wish` is usually -1 to 1 on each axis.
    pub fn character_move(&mut self, id: &str, wish: [f32; 2]) {
        self.characters.command(character2d::Cmd::Move { id: id.to_string(), wish });
    }

    /// Ask a Character 2D to jump on the next play step.
    pub fn character_jump(&mut self, id: &str) {
        self.characters.command(character2d::Cmd::Jump { id: id.to_string() });
    }

    /// Floor, wall and velocity of a Character 2D after the last step.
    pub fn character_status(&self, id: &str) -> Option<character2d::View> {
        self.characters.view(id)
    }

    /// Add one entity under the root. `kind` is a shape name, `camera`, `light`, or `empty`.
    /// Returns the new id.
    /// `base`, or `base 2`, `base 3`... when another object already uses that name.
    pub fn free_name(&self, base: &str) -> String {
        let taken = |name: &str| self.entities.iter().any(|e| e.name == name);
        if !taken(base) {
            return base.to_string();
        }
        // "Coin 3" copied gives "Coin 4", not "Coin 3 2": number after the stem,
        // one past the highest number already used, so names follow creation order.
        let stem = match base.rsplit_once(' ') {
            Some((stem, n)) if !stem.is_empty() && n.parse::<u32>().is_ok() => stem,
            _ => base,
        };
        let highest = self
            .entities
            .iter()
            .filter_map(|e| {
                if e.name == stem {
                    return Some(1);
                }
                e.name.strip_prefix(stem)?.strip_prefix(' ')?.parse::<u32>().ok()
            })
            .max()
            .unwrap_or(1);
        ((highest + 1)..)
            .map(|n| format!("{stem} {n}"))
            .find(|name| !taken(name))
            .unwrap_or_default()
    }

    pub fn add(&mut self, kind: &str) -> String {
        let shape = Shape::parse(kind);
        let (name, tag) = match kind {
            "cube" => ("Cube", "prop"),
            "sphere" => ("Sphere", "prop"),
            "cylinder" => ("Cylinder", "prop"),
            "cone" => ("Cone", "prop"),
            "capsule" => ("Capsule", "prop"),
            "plane" => ("Plane", "ground"),
            "camera" => ("Camera", "camera"),
            "light" => ("Light", "light"),
            "sprite_shape" => ("Sprite Shape", ""),
            "wind_area" => ("Wind Area", ""),
            "water_area" => ("Water Area", ""),
            "gravity_area" => ("Gravity Area", ""),
            "attractor" => ("Attractor", ""),
            "kill_zone" => ("Kill Zone", ""),
            "damage_zone" => ("Damage Zone", ""),
            "checkpoint" => ("Checkpoint", ""),
            "trigger_volume" => ("Trigger Volume", ""),
            "post_process" => ("Post Process", ""),
            "sprite_mask" => ("Sprite Mask", ""),
            "character_2d" => ("Character 2D", ""),
            _ => ("Empty", ""),
        };
        let id_base = if kind == "sprite_mask" {
            "sprite-mask".into()
        } else if kind == "character_2d" {
            "character-2d".into()
        } else {
            name.to_lowercase()
        };
        let id = self.free_id(&id_base);
        let name = self.free_name(name);
        let name = name.as_str();
        let dynamic = matches!(
            shape,
            Some(Shape::Cube | Shape::Sphere | Shape::Cylinder | Shape::Cone | Shape::Capsule)
        );
        let height = if dynamic { 3.0 } else { 0.0 };
        self.entities.push(entity(
            &id,
            name,
            Some("root"),
            [0.0, height, 0.0],
            shape,
            dynamic,
            tag,
        ));
        if kind == "sprite_shape" {
            if let Some(entity) = self.entity_mut(&id) {
                entity.sprite_shape = sprite_shape::starter();
                sprite_shape::sync_collider(entity);
            }
        }
        if let Some(created) = self.entity_mut(&id) {
            area_effect::configure(created, kind);
            if kind == "character_2d" {
                character2d::configure(created);
            }
            if kind == "camera" {
                created.audio_listener.on = true;
            }
            volumes::configure(created, kind);
        }
        if kind == "sprite_mask" {
            if let Some(made) = self.entities.last_mut() {
                made.sprite_mask.shape = "rect".into();
                made.scale = [2.0, 2.0, 1.0];
                made.color = [0.35, 0.72, 0.86];
            }
        }
        id
    }

    /// Remove an entity and everything under it. The root stays.
    pub fn remove(&mut self, id: &str) -> bool {
        if id == "root" || self.entity(id).is_none() {
            return false;
        }
        let mut doomed = vec![id.to_string()];
        let mut index = 0;
        while index < doomed.len() {
            let parent = doomed[index].clone();
            for entity in &self.entities {
                if entity.parent.as_deref() == Some(parent.as_str()) {
                    doomed.push(entity.id.clone());
                }
            }
            index += 1;
        }
        self.entities.retain(|entity| !doomed.contains(&entity.id));
        true
    }

    /// One simulation frame while paused or playing.
    pub fn step_once(&mut self) -> bool {
        if self.mode == PlayMode::Editing {
            return false;
        }
        if self.platformer.is_some() {
            // A full frame: one physics tick plus scripts, animations, sequences and particles.
            if let Some(game) = self.platformer.as_mut() {
                game.clear_carry();
            }
            let tick = self
                .platformer
                .as_ref()
                .map(|g| g.tick_seconds())
                .unwrap_or(platformer::DEFAULT_TICK_SECONDS);
            let mode = self.mode;
            self.mode = PlayMode::Playing;
            let scale = self.time_scale;
            self.time_scale = 1.0;
            self.step(tick);
            self.time_scale = scale;
            if self.mode == PlayMode::Playing {
                self.mode = mode;
            }
            return true;
        }
        #[cfg(not(feature = "play-preview"))]
        {
            false
        }
        #[cfg(feature = "play-preview")]
        {
            if self.sim.is_none() {
                return false;
            }
            let mode = self.mode;
            self.mode = PlayMode::Playing;
            let moved = self.step(1.0 / 60.0);
            self.mode = mode;
            moved
        }
    }

    /// Merge the selected sprites into one batch group, or shapes and meshes into one mesh.
    pub fn merge_objects(&mut self, ids: &[String]) -> Result<String, String> {
        object_merge::merge(self, ids)
    }

    /// Parent the given entities under a new empty.
    pub fn group(&mut self, ids: &[String]) -> String {
        let id = self.add("empty");
        if let Some(entity) = self.entity_mut(&id) {
            entity.name = "Group".into();
        }
        for child in ids {
            if child == "root" || child == &id {
                continue;
            }
            if let Some(entity) = self.entity_mut(child) {
                entity.parent = Some(id.clone());
            }
        }
        id
    }

    /// Move children up and delete the group entity itself.
    pub fn ungroup(&mut self, id: &str) -> bool {
        if id == "root" {
            return false;
        }
        let Some(parent) = self.entity(id).and_then(|entity| entity.parent.clone()) else {
            return false;
        };
        for entity in &mut self.entities {
            if entity.parent.as_deref() == Some(id) {
                entity.parent = Some(parent.clone());
            }
        }
        self.entities.retain(|entity| entity.id != id);
        true
    }

    /// Move the camera onto an entity, then start play.
    pub fn play_from(&mut self, id: &str) -> bool {
        let Some(at) = self.entity(id).map(|entity| entity.translation) else {
            return false;
        };
        if let Some(camera) = self.entity_mut("camera") {
            camera.translation = at;
        }
        self.play();
        true
    }

    /// Put the bottom of the shape on the ground plane.
    pub fn snap_floor(&mut self, id: &str) -> bool {
        let Some(entity) = self.entity(id) else {
            return false;
        };
        let half = match entity.shape {
            Some(Shape::Cube) => 0.5 * entity.scale[1].abs(),
            Some(Shape::Sphere) => 0.5 * entity.scale.into_iter().fold(0.0_f32, f32::max).abs(),
            Some(Shape::Cylinder) => 0.5 * entity.scale[1].abs(),
            Some(Shape::Cone) => 0.5 * entity.scale[1].abs(),
            Some(Shape::Capsule) => 0.5 * entity.scale[1].abs(),
            Some(Shape::Plane) => 0.05 * entity.scale[1].abs(),
            None => 0.0,
        };
        let Some(entity) = self.entity_mut(id) else {
            return false;
        };
        entity.translation[1] = half;
        true
    }

    /// Clear rotation and set the size back to one. The location stays.
    pub fn reset_transform(&mut self, id: &str) -> bool {
        let Some(entity) = self.entity_mut(id) else {
            return false;
        };
        entity.rotation = [0.0, 0.0, 0.0];
        entity.scale = [1.0, 1.0, 1.0];
        true
    }

    /// Copy one entity one unit to the right. Returns the id of the copy.
    pub fn duplicate(&mut self, id: &str) -> Option<String> {
        if id == "root" {
            return None;
        }
        let mut copy = self.entity(id)?.clone();
        copy.id = self.free_id(id.trim_end_matches(|c: char| c.is_ascii_digit() || c == '-'));
        // Step one object-width to the side so the copy is visibly separate.
        copy.translation[0] += copy.scale[0].abs().max(1.0);
        let base = copy.name.trim_end_matches(|c: char| c.is_ascii_digit()).trim_end().to_string();
        copy.name = self.free_name(if base.is_empty() { &copy.name } else { &base });
        let new_id = copy.id.clone();
        self.entities.push(copy);
        Some(new_id)
    }

    /// Text of a prefab holding `root_id` and everything under it (root detached, schema "prefab").
    pub fn prefab_text(&self, root_id: &str) -> Option<String> {
        if root_id == "root" {
            return None;
        }
        self.entity(root_id)?;
        let mut ids = vec![root_id.to_string()];
        let mut i = 0;
        while i < ids.len() {
            let parent = ids[i].clone();
            for e in &self.entities {
                if e.parent.as_deref() == Some(parent.as_str()) && !ids.contains(&e.id) {
                    ids.push(e.id.clone());
                }
            }
            i += 1;
        }
        let rows: Vec<SealValue> = ids
            .iter()
            .filter_map(|id| self.entity(id))
            .map(|e| {
                let mut e = e.clone();
                if e.id == root_id {
                    e.parent = None;
                }
                entity_value(&e)
            })
            .collect();
        Some(
            SealValue::Map(
                vec![
                    ("seal_prefab".into(), SealValue::Number("1".into())),
                    ("root".into(), SealValue::String(root_id.to_string())),
                    ("entities".into(), SealValue::Array(rows)),
                ]
                .into(),
            )
            .to_text(),
        )
    }

    /// Rebuild every copy of prefab `asset` from `text`. Each copy keeps its id, name,
    /// parent, placement and the fields listed in its `overrides`; its children come
    /// fresh from the file. Returns how many copies were rebuilt.
    pub fn refresh_prefab_instances(&mut self, asset: &str, text: &str) -> Result<usize, String> {
        let roots: Vec<String> = self
            .entities
            .iter()
            .filter(|e| e.source_kind == "prefab" && e.source_asset == asset)
            .map(|e| e.id.clone())
            .collect();
        for old_id in &roots {
            let Some(old) = self.entity(old_id).cloned() else { continue };
            let kept = prefab_nest::capture(&self.entities, old_id);
            let index = self.entities.iter().position(|e| e.id == *old_id).unwrap_or(0);
            // Drop the old children; the root row is replaced in place below.
            let children: Vec<String> = self
                .entities
                .iter()
                .filter(|e| e.parent.as_deref() == Some(old_id.as_str()))
                .map(|e| e.id.clone())
                .collect();
            for child in children {
                self.remove(&child);
            }
            let fresh_id = self.add_prefab_text(text, old.parent.as_deref())?;
            let fresh_at = self
                .entities
                .iter()
                .position(|e| e.id == fresh_id)
                .ok_or("rebuild lost its root")?;
            let mut merged = self.entities.remove(fresh_at);
            // Positions are stored in scene space, so carry the children along to the copy.
            let delta = [
                old.translation[0] - merged.translation[0],
                old.translation[1] - merged.translation[1],
                old.translation[2] - merged.translation[2],
            ];
            self.shift_children(&fresh_id, delta);
            for field in old.overrides.split(',').map(str::trim).filter(|f| !f.is_empty()) {
                copy_field(&old, &mut merged, field);
            }
            merged.id = old.id.clone();
            merged.name = old.name.clone();
            merged.parent = old.parent.clone();
            merged.translation = old.translation;
            merged.rotation = old.rotation;
            merged.scale = old.scale;
            merged.hidden = old.hidden;
            merged.locked = old.locked;
            merged.unselectable = old.unselectable;
            merged.source_kind = old.source_kind.clone();
            merged.source_asset = old.source_asset.clone();
            merged.overrides = old.overrides.clone();
            for e in &mut self.entities {
                if e.parent.as_deref() == Some(fresh_id.as_str()) {
                    e.parent = Some(old.id.clone());
                }
            }
            let at = self
                .entities
                .iter()
                .position(|e| e.id == *old_id)
                .unwrap_or(index.min(self.entities.len()));
            self.entities[at] = merged;
            prefab_nest::restore(&mut self.entities, old_id, &kept);
        }
        Ok(roots.len())
    }

    /// Move everything under `id` (not `id` itself) by `delta`.
    pub fn shift_children(&mut self, id: &str, delta: [f32; 3]) {
        let mut under = vec![id.to_string()];
        let mut i = 0;
        while i < under.len() {
            let parent = under[i].clone();
            for e in &self.entities {
                if e.parent.as_deref() == Some(parent.as_str()) && !under.contains(&e.id) {
                    under.push(e.id.clone());
                }
            }
            i += 1;
        }
        for e in &mut self.entities {
            if e.id != id && under.contains(&e.id) {
                for (axis, d) in delta.iter().enumerate() {
                    e.translation[axis] += d;
                }
            }
        }
    }

    /// Add a prefab's objects under `parent` (default the root) with fresh ids and names.
    /// Returns the new root id.
    pub fn add_prefab_text(&mut self, text: &str, parent: Option<&str>) -> Result<String, String> {
        let prefabs_dir = self.project_dir.as_deref().map(|d| d.join("prefabs"));
        let text = match prefabs_dir.as_deref() {
            Some(dir) => crate::prefab_variant::read_prefab_for_world(Some(dir), text, false)?,
            None => text.to_string(),
        };
        let value = parse(&text).map_err(|err| err.to_string())?;
        if value.get("seal_prefab").is_none() {
            return Err("not a prefab file".into());
        }
        let Some(SealValue::Array(items)) = value.get("entities") else {
            return Err("prefab is missing entities".into());
        };
        let mut rows = Vec::new();
        for item in items {
            rows.push(entity_from(item)?);
        }
        let Some(first) = rows.first() else { return Err("prefab has no objects".into()) };
        let old_root = match value.get("root") {
            Some(SealValue::String(s)) => s.clone(),
            _ => first.id.clone(),
        };
        let parent = parent.filter(|p| self.entity(p).is_some()).unwrap_or("root").to_string();
        let mut map: Vec<(String, String)> = Vec::new();
        for e in &rows {
            let base = e.id.trim_end_matches(|c: char| c.is_ascii_digit() || c == '-').to_string();
            let mut id = self.free_id(if base.is_empty() { "object" } else { &base });
            while map.iter().any(|(_, n)| *n == id) {
                id.push('x');
                id = self.free_id(&id);
            }
            map.push((e.id.clone(), id));
        }
        let new_root = map
            .iter()
            .find(|(o, _)| *o == old_root)
            .map(|(_, n)| n.clone())
            .ok_or("prefab root missing")?;
        for mut e in rows {
            let old = e.id.clone();
            e.id =
                map.iter().find(|(o, _)| *o == old).map(|(_, n)| n.clone()).unwrap_or(old.clone());
            e.parent = if old == old_root {
                Some(parent.clone())
            } else {
                e.parent
                    .and_then(|p| map.iter().find(|(o, _)| *o == p).map(|(_, n)| n.clone()))
                    .or(Some(new_root.clone()))
            };
            if old == old_root {
                e.name = self.free_name(&e.name);
            }
            self.entities.push(e);
        }
        Ok(new_root)
    }

    fn free_id(&self, base: &str) -> String {
        if self.entity(base).is_none() {
            return base.to_string();
        }
        (2..)
            .map(|n| format!("{base}-{n}"))
            .find(|id| self.entity(id).is_none())
            .unwrap_or_default()
    }

    /// Coins, falls and whether the level is clear, while side-view gameplay runs.
    /// Text lines the playing scripts' on-screen widgets show.
    pub fn play_ui_lines(&self) -> Vec<String> {
        // Widgets drawn from a screen file do not also show as text lines.
        if self.game_screens.active() && !self.game_screens.uses_lines() {
            return Vec::new();
        }
        self.scripts.as_ref().map(|s| s.ui.lines()).unwrap_or_default()
    }

    /// The game rules scripts set while playing (who controls what, scores, state).
    pub fn play_game(&self) -> Option<&game::Game> {
        if let Some(scripts) = self.scripts.as_ref() {
            return Some(&scripts.game);
        }
        (self.mode != PlayMode::Editing).then_some(&self.rules)
    }

    pub fn play_game_mut(&mut self) -> Option<&mut game::Game> {
        if let Some(scripts) = self.scripts.as_mut() {
            return Some(&mut scripts.game);
        }
        (self.mode != PlayMode::Editing).then_some(&mut self.rules)
    }

    pub fn play_status(&self) -> Option<PlayStatus> {
        self.platformer.as_ref().map(|game| PlayStatus {
            coins: game.coins_got,
            coins_total: game.coins_total,
            falls: game.falls,
            won: game.won,
            goal_locked: game.goal_hint > 0.0,
        })
    }

    /// The scene id of the object side-view gameplay controls.
    /// Where the player stood when play started.
    /// Last-step 2D physics counters during play, when a 2D simulation is running.
    pub fn physics2d_profiler(&self) -> Option<&seal_physics2d::Physics2DProfiler> {
        self.sim2d.as_ref().map(|s| s.profiler())
    }

    pub fn play_start(&self) -> Option<[f32; 2]> {
        self.platformer.as_ref().map(|game| game.start())
    }

    /// Whether any object script runs in this play.
    pub fn has_scripts(&self) -> bool {
        self.scripts.is_some()
    }

    /// Fire a script event on every running graph (for tests and tooling).
    pub fn fire_script_event(&mut self, event: &str) {
        if let Some(scripts) = self.scripts.as_mut() {
            scripts.fire(event, &mut self.entities, &mut self.play_log);
        }
    }

    pub fn play_player(&self) -> Option<&str> {
        self.platformer.as_ref().map(|game| game.player_id())
    }

    /// The play camera a 3D view looks through during play: the visible object
    /// tagged `camera` with a field of view. Gives its position, rotation and fov.
    pub fn play_look_camera(&self) -> Option<([f32; 3], [f32; 3], f32)> {
        if self.mode == PlayMode::Editing || self.platformer.is_some() {
            return None;
        }
        self.entities
            .iter()
            .find(|e| e.tag == "camera" && !e.hidden && e.fov > 0.0)
            .map(|e| (e.translation, e.rotation, e.fov))
    }

    /// Set each frame by the view: true while it looks through the play camera.
    pub fn set_look_camera(&mut self, on: bool) {
        self.look_camera = on;
    }

    /// Where a scripted (non-platformer) play should keep the view centred: the
    /// scene's camera object if it has one, else its player. Scripts move either
    /// to steer the view. `None` while editing or when neither exists.
    pub fn play_view_target(&self) -> Option<[f32; 3]> {
        if self.mode == PlayMode::Editing || self.platformer.is_some() {
            return None;
        }
        let pick = |tag: &str| {
            self.entities.iter().find(|e| e.tag == tag && !e.hidden).map(|e| e.translation)
        };
        pick("camera").or_else(|| pick("player"))
    }

    /// Start or resume. The first start keeps a copy so stop can put the scene back.
    /// Ray cast against the play physics: first object hit and distance.
    #[cfg(feature = "play-preview")]
    pub fn ray_cast(&self, from: [f32; 3], dir: [f32; 3], max: f32) -> Option<(String, f32)> {
        self.sim.as_ref()?.ray_cast(from, dir, max)
    }

    /// Move every character (walk_speed above 0) from the held keys: forward is -z,
    /// right is +x, Space jumps.
    #[cfg(feature = "play-preview")]
    fn drive_characters(&mut self) {
        let Some(scripts) = self.scripts.as_ref() else { return };
        let (fwd, right, jump) = (
            scripts.axis_now("MoveForward") as f32,
            scripts.axis_now("MoveRight") as f32,
            scripts.is_held("SpaceBar"),
        );
        // Once a script possessed an object for player 0, keys drive only that one.
        let only = scripts.game.possessed.get(&0).cloned();
        let ids: Vec<(String, f32)> = self
            .entities
            .iter()
            .filter(|e| e.dynamic && e.walk_speed > 0.0 && only.as_ref().is_none_or(|o| *o == e.id))
            .map(|e| (e.id.clone(), e.walk_speed))
            .collect();
        let rules = CharacterRules::default();
        let yaw = self
            .look_camera
            .then(|| self.play_look_camera().map(|(_, rotation, _)| rotation[1]))
            .flatten();
        let wish = camera_wish(yaw, fwd, right);
        for (id, speed) in ids {
            self.character_step(&id, [wish[0] * speed, wish[1] * speed], jump, &rules);
        }
    }

    /// Walk, jump and climb a capsule character; returns whether it stands on something.
    #[cfg(feature = "play-preview")]
    pub fn character_step(
        &mut self,
        id: &str,
        wish: [f32; 2],
        jump: bool,
        rules: &CharacterRules,
    ) -> bool {
        let Some(sim) = self.sim.as_mut() else { return false };
        let grounded = sim.character_step(id, wish, jump, rules);
        // The next step copies entity speed into the body, so hand it the new state.
        if let (Some((p, v)), Some(e)) =
            (sim.pose(id), self.entities.iter_mut().find(|e| e.id == id))
        {
            e.translation = p;
            e.velocity = v;
        }
        grounded
    }

    /// Push a dynamic object during play.
    #[cfg(feature = "play-preview")]
    pub fn apply_impulse(&mut self, id: &str, impulse: [f32; 3]) {
        if let Some(sim) = self.sim.as_mut() {
            sim.apply_impulse(id, impulse);
        }
    }

    pub fn play(&mut self) {
        let was_paused = self.mode == PlayMode::Paused;
        if self.mode == PlayMode::Editing {
            self.quit = false;
            self.snapshot = Some(self.entities.clone());
            self.camera_rig.clear();
            self.play_log.clear();
            self.play_log.extend(self.physics.begin_play(
                &self.entities,
                self.project_dir.as_deref(),
                self.gravity,
            ));
            self.install_builtin_sims();
            self.sequence_warned.clear();
            self.play_time = 0.0;
            self.unscaled_time = 0.0;
            self.unscaled_dt = 0.0;
            self.sim_tick = 0;
            self.anim_cursor.clear();
            let raw = std::mem::take(&mut self.controls.raw);
            self.controls =
                controls::Controls::load(self.project_dir.as_deref(), &mut self.play_log);
            // Keys and pad buttons already down stay down; the window keeps feeding them.
            self.controls.raw = raw;
            if let Some(dir) = self.project_dir.clone() {
                self.scripts = scripts::Scripts::start(
                    &mut self.entities,
                    Some(&dir),
                    &self.breakpoints,
                    &mut self.play_log,
                    std::sync::Arc::clone(&self.physics.server),
                );
                self.start_scene_emitters(&dir, None);
                self.refresh_save_slots();
            }
            self.volume_memory = volumes::Memory::default();
            self.join_roster = players::Roster::default();
            self.rules = game::Game::default();
            self.queue_scene_music();
            self.scenes.begin();
            self.share_scenes();
            #[cfg(feature = "play-preview")]
            if self.platformer.is_none() && self.sim2d.is_none() {
                self.sim = Some(self.open_preview_sim());
            }
        }
        if self.mode == PlayMode::Paused {
            // Resuming from a breakpoint first finishes the stopped run.
            if let Some(scripts) = self.scripts.as_mut() {
                if scripts.debug_continue(false, &mut self.entities, &mut self.play_log) {
                    return;
                }
            }
        }
        self.mode =
            if self.at_breakpoint().is_some() { PlayMode::Paused } else { PlayMode::Playing };
        if was_paused && self.mode == PlayMode::Playing {
            self.play_music.push(scripts::MusicCue::Resume);
        } else if !was_paused && self.mode == PlayMode::Paused {
            self.play_music.push(scripts::MusicCue::Pause);
        }
    }

    /// Queue the scene's music file, looping from the start, when play begins.
    fn queue_scene_music(&mut self) {
        let file = self.music.trim();
        if file.is_empty() {
            return;
        }
        self.play_music.push(scripts::MusicCue::Play {
            file: file.to_string(),
            looping: true,
            fade: 0.0,
            loop_start: 0.0,
        });
    }

    /// Where play is stopped at a breakpoint, if it is.
    pub fn at_breakpoint(&self) -> Option<&scripts::BreakHit> {
        self.scripts.as_ref()?.hit.as_ref().map(|(_, h)| h)
    }

    /// Run one node of the run stopped at a breakpoint. Returns whether it is still stopped.
    pub fn debug_step(&mut self) -> bool {
        let Some(scripts) = self.scripts.as_mut() else { return false };
        let still = scripts.debug_continue(true, &mut self.entities, &mut self.play_log);
        if let Some(scripts) = self.scripts.as_mut() {
            scripts.follow(&mut self.entities);
            self.play_sounds.append(&mut scripts.sounds);
            self.play_music.append(&mut scripts.music);
        }
        self.place_sockets();
        still
    }

    /// Set the breakpoints of graph file `path`, also in a running play.
    pub fn set_breakpoints(&mut self, path: &str, nodes: std::collections::BTreeSet<u32>) {
        if let Some(scripts) = self.scripts.as_mut() {
            scripts.set_breakpoints(path, &nodes);
        }
        if nodes.is_empty() {
            self.breakpoints.remove(path);
        } else {
            self.breakpoints.insert(path.to_string(), nodes);
        }
    }

    /// What graph file `path` is doing in the running play.
    pub fn debug_view(&self, path: &str) -> Option<scripts::DebugView> {
        self.scripts.as_ref()?.debug_view(path, &self.entities)
    }

    pub fn pause(&mut self) {
        if self.mode == PlayMode::Playing {
            self.mode = PlayMode::Paused;
            self.play_music.push(scripts::MusicCue::Pause);
        }
    }

    pub fn stop(&mut self) {
        let was_playing = self.mode != PlayMode::Editing;
        if let Some(snapshot) = self.snapshot.take() {
            self.entities = snapshot;
        }
        #[cfg(feature = "play-preview")]
        {
            self.sim = None;
        }
        self.platformer = None;
        self.sim2d = None;
        self.physics.end_play();
        self.scripts = None;
        self.camera_rig.clear();
        self.play_keys.clear();
        self.game_screens.clear();
        self.play_sounds.clear();
        self.play_music.clear();
        if was_playing {
            self.play_music.push(scripts::MusicCue::Stop { fade: 0.0 });
        }
        self.play_time = 0.0;
        self.unscaled_time = 0.0;
        self.unscaled_dt = 0.0;
        self.sim_tick = 0;
        self.anim_cursor.clear();
        self.anims.clear();
        self.sequences.clear();
        self.sequence_warned.clear();
        self.emitters.clear();
        self.net = None;
        self.sequence_events.clear();
        self.ai = ai::Ai::default();
        self.walkers = nav2d::Walkers::default();
        self.followers = shape2d::Followers::default();
        self.characters = character2d::Motion::default();
        self.play_input = PlayInput::default();
        self.volume_memory = volumes::Memory::default();
        self.join_roster = players::Roster::default();
        self.rules = game::Game::default();
        self.controls.rumble.stop();
        self.controls.motors = (0.0, 0.0);
        self.controls.capture = None;
        self.controls.rebound.clear();
        self.scenes.end();
        self.mode = PlayMode::Editing;
    }

    /// A light-bar colour a script asked for, channels 0 to 1.
    pub fn take_pad_light(&mut self) -> Option<[f32; 3]> {
        self.scripts.as_mut().and_then(|scripts| scripts.ui.pad_light.take())
    }

    /// Turn the device state in `controls.raw` into this frame's actions and
    /// side-view moves. The window calls it once a frame while playing.
    pub fn read_controls(&mut self) {
        if self.mode != PlayMode::Playing {
            return;
        }
        self.play_input = self.controls.frame();
        if self.join_on {
            let edges = self.controls.take_start_edges();
            for pad in edges {
                if let Some(scripts) = self.scripts.as_mut() {
                    players::join_pad(
                        &mut scripts.game,
                        &mut self.join_roster,
                        &self.entities,
                        pad,
                    );
                } else {
                    players::join_pad(&mut self.rules, &mut self.join_roster, &self.entities, pad);
                }
            }
        }
    }

    /// Hand scripts this frame's actions and carry out what they asked of the
    /// pad: rumbles and rebinding waits; tell them when a rebind finished.
    fn exchange_controls(&mut self, dt: f32) {
        let Some(scripts) = self.scripts.as_mut() else {
            self.controls.step(dt);
            return;
        };
        let ui = &mut scripts.ui;
        for (strong, weak, secs) in ui.rumbles.drain(..) {
            self.controls.play_rumble(strong, weak, secs);
        }
        for (action, slot) in std::mem::take(&mut ui.rebinds) {
            if let Err(e) = self.controls.start_rebind(&action, slot) {
                self.play_log.push(e);
            }
        }
        self.controls.step(dt);
        for (action, result) in std::mem::take(&mut self.controls.rebound) {
            let how = match &result {
                seal_input::Capture::Bound(b) => {
                    format!("{action} is now {}", seal_input::button_label(b))
                }
                seal_input::Capture::Cancelled => format!("rebinding {action} was cancelled"),
            };
            self.play_log.push(how);
            scripts.fire("input_rebound", &mut self.entities, &mut self.play_log);
        }
        let ui = &mut scripts.ui;
        ui.actions = self
            .controls
            .map
            .actions()
            .map(|(name, _)| (name.to_string(), self.controls.state.pressed(name)))
            .collect();
        ui.axes = self
            .controls
            .map
            .axes()
            .map(|(name, _)| (name.to_string(), self.controls.state.axis(name)))
            .collect();
        ui.held_for = self
            .controls
            .map
            .actions()
            .map(|(name, _)| (name.to_string(), self.controls.state.held_seconds(name)))
            .collect();
        ui.released = self
            .controls
            .map
            .actions()
            .map(|(name, _)| (name.to_string(), self.controls.state.just_released(name)))
            .collect();
        ui.mouse_delta = self.controls.mouse_delta;
        ui.raw_mouse = self.controls.raw_mouse;
        ui.look_delta = self.controls.look_delta;
        ui.pen_pressure = self.controls.pen_pressure;
        ui.pen_tilt = self.controls.pen_tilt;
        ui.pen_eraser = self.controls.pen_eraser;
        ui.pad_gyro = self.controls.pad_gyro;
        ui.pad_accel = self.controls.pad_accel;
        ui.pad_motion_note.clone_from(&self.controls.pad_motion_note);
        ui.pad_light_note.clone_from(&self.controls.pad_light_note);
        ui.mouse_wheel = self.controls.mouse_wheel;
        ui.pointer_world = self.controls.pointer_world;
    }

    /// Carry out the saves, loads and deletes scripts asked for last step.
    fn exchange_saves(&mut self) {
        use seal_graph::rt::GameCmd;
        let Some(scripts) = self.scripts.as_mut() else { return };
        let cmds = std::mem::take(&mut scripts.ui.saves);
        if cmds.is_empty() {
            return;
        }
        let Some(dir) = self.project_dir.clone() else {
            self.play_log.push("saving needs a project folder".into());
            return;
        };
        for cmd in cmds {
            let result = match cmd {
                GameCmd::SaveGame { slot } => self.save_game(&dir, &slot).map(|_| ()),
                GameCmd::LoadGame { slot } => self.load_game(&dir, &slot),
                GameCmd::DeleteSave { slot } => savegame::delete(&dir, &slot).map(|had| {
                    if had {
                        self.play_log.push(format!("deleted the save {}", slot.trim()));
                    }
                }),
                _ => Ok(()),
            };
            if let Err(e) = result {
                self.play_log.push(e);
            }
        }
        self.refresh_save_slots();
    }

    /// Carry out the scene requests scripts made last step and switch to
    /// background loads that finished.
    fn exchange_scenes(&mut self) {
        use seal_graph::rt::GameCmd;
        let (cmds, keeps) = match self.scripts.as_mut() {
            Some(s) => (std::mem::take(&mut s.ui.scene_cmds), std::mem::take(&mut s.ui.keeps)),
            None => (Vec::new(), Vec::new()),
        };
        for (id, keep) in keeps {
            if keep {
                self.scenes.kept.insert(id);
            } else {
                self.scenes.kept.remove(&id);
            }
        }
        let mut changed = false;
        if !cmds.is_empty() {
            let Some(dir) = self.project_dir.clone() else {
                self.play_log.push("opening scenes needs a project folder".into());
                return;
            };
            for cmd in cmds {
                let result =
                    match cmd {
                        GameCmd::OpenScene { path } => scenes::read(&dir, &path)
                            .map(|objs| self.open_scene_objects(&path, objs)),
                        GameCmd::AddScene { path } => scenes::read(&dir, &path)
                            .map(|objs| self.add_scene_objects(&path, objs)),
                        GameCmd::LoadSceneAsync { path, additive } => {
                            let r = self.scenes.load_async(&dir, &path, additive);
                            if r.is_ok() {
                                self.play_log.push(format!(
                                    "loading {} in the background",
                                    scenes::normal(&path)
                                ));
                            }
                            r
                        }
                        GameCmd::UnloadScene { path } => {
                            self.unload_scene(&path);
                            Ok(())
                        }
                        _ => Ok(()),
                    };
                match result {
                    Ok(()) => changed = true,
                    Err(e) => self.play_log.push(e),
                }
            }
        }
        for (path, additive, result) in self.scenes.finished() {
            match result {
                Ok(objs) if additive => self.add_scene_objects(&path, objs),
                Ok(objs) => self.open_scene_objects(&path, objs),
                Err(e) => self.play_log.push(e),
            }
            changed = true;
        }
        if changed {
            self.share_scenes();
        }
    }

    /// Put the objects of scene `path` in place of the running scene, keeping
    /// kept objects (and what is under them), then fire `scene_loaded`.
    pub fn open_scene_objects(&mut self, path: &str, mut objs: Vec<Entity>) {
        let path = scenes::normal(path);
        let keep = scenes::with_children(&self.entities, &self.scenes.kept);
        let remove: std::collections::BTreeSet<String> =
            self.entities.iter().map(|e| e.id.clone()).filter(|id| !keep.contains(id)).collect();
        // A kept object wins over the new scene's object of the same id, so a
        // player carried into a level that also places one stays single.
        let before = objs.len();
        objs.retain(|e| !keep.contains(&e.id));
        let skipped = before - objs.len();
        let ids: std::collections::BTreeSet<String> = objs.iter().map(|e| e.id.clone()).collect();
        if let Some(scripts) = self.scripts.as_mut() {
            scripts.anim_cmds.retain(|(id, _)| !remove.contains(id));
        }
        self.swap_objects(&remove, objs);
        // Kept objects whose parent went with the old scene move to the top.
        let ids_now: std::collections::BTreeSet<String> =
            self.entities.iter().map(|e| e.id.clone()).collect();
        for e in self.entities.iter_mut().filter(|e| keep.contains(&e.id)) {
            if e.parent.as_ref().is_some_and(|p| !ids_now.contains(p)) {
                e.parent = None;
            }
        }
        self.scenes.open = path.clone();
        self.scenes.added.clear();
        self.scenes.kept.retain(|id| keep.contains(id));
        let mut note = format!("opened {path}");
        if !keep.is_empty() {
            note.push_str(&format!(" ({} kept)", keep.len()));
        }
        if skipped > 0 {
            note.push_str(&format!("; {skipped} of its objects were already kept and left out"));
        }
        self.play_log.push(note);
        self.after_scene_change(&ids);
    }

    /// Add the objects of scene `path` next to the running scene and fire `scene_loaded`.
    pub fn add_scene_objects(&mut self, path: &str, mut objs: Vec<Entity>) {
        let path = scenes::normal(path);
        if self.scenes.is_loaded(&path) {
            self.unload_scene(&path);
        }
        let taken = self.entities.iter().map(|e| e.id.clone()).collect();
        let ids = scenes::fit_ids(&mut objs, &taken, &path);
        let set: std::collections::BTreeSet<String> = ids.iter().cloned().collect();
        self.swap_objects(&Default::default(), objs);
        self.play_log.push(format!("added {path} ({} objects)", ids.len()));
        self.scenes.added.insert(path, ids);
        self.after_scene_change(&set);
    }

    /// Take out the objects scene `path` was added with; kept ones stay.
    pub fn unload_scene(&mut self, path: &str) {
        let path = scenes::normal(path);
        let Some(ids) = self.scenes.added.remove(&path) else {
            self.play_log.push(format!("{path} was not added, so there is nothing to unload"));
            return;
        };
        let keep = scenes::with_children(&self.entities, &self.scenes.kept);
        let ids: std::collections::BTreeSet<String> =
            ids.into_iter().filter(|id| !keep.contains(id)).collect();
        let remove = scenes::with_children(&self.entities, &ids);
        let remove: std::collections::BTreeSet<String> =
            remove.into_iter().filter(|id| !keep.contains(id)).collect();
        self.swap_objects(&remove, Vec::new());
        self.play_log.push(format!("unloaded {path}"));
        self.rebuild_play();
        self.share_scenes();
    }

    fn swap_objects(&mut self, remove: &std::collections::BTreeSet<String>, add: Vec<Entity>) {
        if let Some(scripts) = self.scripts.as_mut() {
            scripts.swap_objects(remove, add, &mut self.entities, &mut self.play_log);
            scripts.follow(&mut self.entities);
        } else {
            self.entities.retain(|e| !remove.contains(&e.id));
            let had_scripts = add.iter().any(|e| !e.script.trim().is_empty());
            self.entities.extend(add);
            if had_scripts {
                self.scripts = scripts::Scripts::start(
                    &mut self.entities,
                    self.project_dir.as_deref(),
                    &self.breakpoints,
                    &mut self.play_log,
                    std::sync::Arc::clone(&self.physics.server),
                );
                self.refresh_save_slots();
            }
        }
        self.emitters.retain(|(id, _)| !remove.contains(id));
        self.anims.retain(|(id, _, _)| !remove.contains(id));
        self.place_sockets();
    }

    fn after_scene_change(&mut self, fresh: &std::collections::BTreeSet<String>) {
        self.rebuild_play();
        if let Some(dir) = self.project_dir.clone() {
            self.start_scene_emitters(&dir, Some(fresh));
        }
        self.share_scenes();
        if let Some(scripts) = self.scripts.as_mut() {
            scripts.fire("scene_loaded", &mut self.entities, &mut self.play_log);
        }
    }

    /// New physics and movers for the objects now in play.
    fn rebuild_play(&mut self) {
        self.camera_rig.clear();
        self.ai = ai::Ai::default();
        self.walkers = nav2d::Walkers::default();
        self.followers = shape2d::Followers::default();
        self.characters = character2d::Motion::default();
        self.play_log.extend(self.physics.begin_play(
            &self.entities,
            self.project_dir.as_deref(),
            self.gravity,
        ));
        self.install_builtin_sims();
        #[cfg(feature = "play-preview")]
        {
            self.sim = (self.platformer.is_none() && self.sim2d.is_none())
                .then(|| self.open_preview_sim());
        }
    }

    /// Objects kept out of the ordinary play simulation, when any are assigned to a physics world.
    fn held_out_of_builtin(&self) -> Option<Vec<Entity>> {
        let skip = self.physics.linked_ids();
        if skip.is_empty() {
            None
        } else {
            Some(
                self.entities
                    .iter()
                    .filter(|entity| !skip.iter().any(|id| id == &entity.id))
                    .cloned()
                    .collect(),
            )
        }
    }

    fn install_builtin_sims(&mut self) {
        if let Some(bodies) = self.held_out_of_builtin() {
            self.platformer =
                platformer::Platformer::from_entities(&bodies, self.gravity, &self.phys2d_settings);
            self.sim2d = match self.platformer {
                Some(_) => None,
                None => phys2d::Sim2d::from_entities(
                    &bodies,
                    self.gravity,
                    self.phys2d_settings.clone(),
                ),
            };
        } else {
            self.platformer = platformer::Platformer::from_entities(
                &self.entities,
                self.gravity,
                &self.phys2d_settings,
            );
            self.sim2d = match self.platformer {
                Some(_) => None,
                None => phys2d::Sim2d::from_entities(
                    &self.entities,
                    self.gravity,
                    self.phys2d_settings.clone(),
                ),
            };
        }
    }

    #[cfg(feature = "play-preview")]
    fn open_preview_sim(&mut self) -> sim::Sim {
        let (solids, notes) = self.physics.preview_solids(self.project_dir.as_deref());
        self.play_log.extend(notes);
        if let Some(bodies) = self.held_out_of_builtin() {
            if solids.is_empty() {
                sim::Sim::from_entities(&bodies)
            } else {
                sim::Sim::from_parts(&bodies, &solids)
            }
        } else if solids.is_empty() {
            sim::Sim::from_entities(&self.entities)
        } else {
            sim::Sim::from_parts(&self.entities, &solids)
        }
    }

    /// Tell scripts which scenes are in the play.
    fn share_scenes(&mut self) {
        let list = self.scenes.loaded();
        if let Some(scripts) = self.scripts.as_mut() {
            scripts.ui.scenes = list;
        }
    }

    /// Tell scripts which save slots hold a game.
    fn refresh_save_slots(&mut self) {
        let (Some(scripts), Some(dir)) = (self.scripts.as_mut(), self.project_dir.as_deref())
        else {
            return;
        };
        scripts.ui.slots = savegame::list(dir)
            .into_iter()
            .filter(|s| s.problem.is_none())
            .map(|s| s.slot)
            .collect();
    }

    /// Write the running game to `slot` in project folder `dir`.
    pub fn save_game(
        &mut self,
        dir: &std::path::Path,
        slot: &str,
    ) -> Result<std::path::PathBuf, String> {
        let game = self.scripts.as_ref().map(|s| s.game.clone()).unwrap_or_default();
        let save = savegame::SaveGame {
            slot: savegame::slot_name(slot)?,
            format: savegame::SAVE_FORMAT,
            game_version: savegame::project_save_version(dir),
            saved_at: savegame::now(),
            play_time: self.play_time,
            entities: self.entities.clone(),
            scores: game.scores,
            state: game.state,
            possessed: game.possessed,
            values: game.values,
            scene: self.scenes.open.clone(),
        };
        let path = savegame::write(dir, &save)?;
        self.play_log.push(format!("saved the game in {}", save.slot));
        if savegame::project_flag(dir, "cloud_save") {
            if let Some(scripts) = self.scripts.as_mut() {
                scripts.ui.cloud.push(cloud::Cmd::Upload(save.slot.clone()));
            }
        }
        Ok(path)
    }

    /// Carry out cloud uploads, downloads and script-hook replies.
    fn exchange_cloud(&mut self) {
        let Some(dir) = self.project_dir.clone() else { return };
        let cmds =
            self.scripts.as_mut().map(|s| std::mem::take(&mut s.ui.cloud)).unwrap_or_default();
        if cmds.is_empty() {
            return;
        }
        if !savegame::project_flag(&dir, "cloud_save") {
            self.play_log.push("turn on cloud saves in Project > Save Games".into());
            return;
        }
        let mut queue = match cloud::load(&dir) {
            Ok(queue) => queue,
            Err(err) => {
                self.play_log.push(err);
                return;
            }
        };
        let mut flush = false;
        let mut replies = Vec::new();
        for cmd in cmds {
            match cmd {
                cloud::Cmd::Upload(slot) => match savegame::slot_path(&dir, &slot) {
                    Ok(path) => match std::fs::read_to_string(&path) {
                        Ok(body) => {
                            let saved_at = cloud::local_saved_at(&dir, &slot).unwrap_or(0);
                            cloud::enqueue_upload(&mut queue, &slot, saved_at, body);
                            flush = true;
                        }
                        Err(err) => {
                            self.play_log.push(format!("cannot read the save {slot}: {err}"))
                        }
                    },
                    Err(err) => self.play_log.push(err),
                },
                cloud::Cmd::Download(slot) => {
                    if let Err(err) = savegame::slot_name(&slot) {
                        self.play_log.push(err);
                        continue;
                    }
                    cloud::enqueue_download(&mut queue, &slot);
                    flush = true;
                }
                cloud::Cmd::Flush => flush = true,
                cloud::Cmd::Reply { slot, saved_at, body } => replies.push((slot, saved_at, body)),
            }
        }
        for (slot, saved_at, body) in replies {
            let local = cloud::local_saved_at(&dir, &slot);
            match cloud::reply(&mut queue, &slot, saved_at, &body, local) {
                Ok(note) => self.apply_cloud_note(&dir, &queue, note),
                Err(err) => self.play_log.push(err),
            }
        }
        if flush {
            let provider = savegame::project_text(&dir, "cloud_provider");
            let url = savegame::project_text(&dir, "cloud_url");
            let notes = if provider == "http" {
                cloud::flush(
                    &mut queue,
                    &|slot| cloud::local_saved_at(&dir, slot),
                    cloud::Mode::Http { url: &url },
                )
            } else {
                cloud::flush(
                    &mut queue,
                    &|slot| cloud::local_saved_at(&dir, slot),
                    cloud::Mode::Script,
                )
            };
            for note in notes {
                self.apply_cloud_note(&dir, &queue, note);
            }
        }
        if let Err(err) = cloud::store(&dir, &queue) {
            self.play_log.push(err);
        }
    }

    fn apply_cloud_note(&mut self, dir: &std::path::Path, queue: &cloud::Queue, note: cloud::Note) {
        match note {
            cloud::Note::Uploaded { slot } => {
                self.play_log.push(format!("uploaded the save {slot}"));
                self.fire_cloud("cloud_uploaded");
            }
            cloud::Note::Downloaded { slot, body } => {
                if let Err(err) = cloud::write_downloaded(dir, &slot, &body) {
                    self.play_log.push(err);
                    return;
                }
                self.play_log.push(format!("downloaded the save {slot}"));
                self.fire_cloud("cloud_downloaded");
            }
            cloud::Note::Conflict { slot, kept } => {
                if let Some(scripts) = self.scripts.as_mut() {
                    scripts.ui.cloud_kept = kept.clone();
                }
                self.play_log.push(format!("cloud save {slot} kept the {kept} copy"));
                let event =
                    if kept == "remote" { "cloud_conflict_remote" } else { "cloud_conflict_local" };
                self.fire_cloud(event);
            }
            cloud::Note::Waiting { slot, message } => {
                self.play_log.push(format!("cloud save {slot} is waiting: {message}"));
            }
            cloud::Note::Hook { slot, .. } => {
                if let Some(job) = cloud::job(queue, &slot).cloned() {
                    if let Some(scripts) = self.scripts.as_mut() {
                        scripts.ui.cloud_job = Some(job);
                    }
                }
                self.fire_cloud("cloud_hook");
            }
        }
    }

    fn fire_cloud(&mut self, event: &str) {
        if let Some(scripts) = self.scripts.as_mut() {
            scripts.fire(event, &mut self.entities, &mut self.play_log);
        }
    }

    /// Put the game saved in `slot` back into the running play and fire `game_loaded`.
    pub fn load_game(&mut self, dir: &std::path::Path, slot: &str) -> Result<(), String> {
        let save = savegame::read(dir, slot)?;
        let current = savegame::project_save_version(dir);
        self.entities = save.entities;
        self.play_time = save.play_time;
        self.camera_rig.clear();
        self.anims.clear();
        self.ai = ai::Ai::default();
        self.walkers = nav2d::Walkers::default();
        self.characters = character2d::Motion::default();
        self.play_log.extend(self.physics.begin_play(
            &self.entities,
            self.project_dir.as_deref(),
            self.gravity,
        ));
        self.install_builtin_sims();
        #[cfg(feature = "play-preview")]
        if self.sim.is_some() {
            self.sim = Some(self.open_preview_sim());
        }
        let note = if save.game_version < current {
            format!(
                "loaded {} (saved at game version {}; the game is at {current})",
                save.slot, save.game_version
            )
        } else {
            format!("loaded {}", save.slot)
        };
        self.play_log.push(note);
        if !save.scene.is_empty() {
            self.scenes.open = save.scene.clone();
        }
        // Restored objects need their graphs again: a pickup that was gone
        // when the scene changed comes back with the save.
        if self.scripts.is_some() {
            if let Some(dir) = self.project_dir.clone() {
                self.scripts = scripts::Scripts::start(
                    &mut self.entities,
                    Some(&dir),
                    &self.breakpoints,
                    &mut self.play_log,
                    std::sync::Arc::clone(&self.physics.server),
                );
            }
        }
        if let Some(scripts) = self.scripts.as_mut() {
            scripts.game.scores = save.scores;
            scripts.game.state = save.state;
            scripts.game.possessed = save.possessed;
            scripts.game.values = save.values;
            scripts.game.values.insert(savegame::VERSION_KEY.into(), save.game_version.to_string());
            scripts.fire("game_loaded", &mut self.entities, &mut self.play_log);
        }
        self.share_scenes();
        Ok(())
    }

    /// Advance the rigid-body simulation. Returns whether the scene was stepped.
    pub fn step(&mut self, dt: f32) -> bool {
        if self.mode != PlayMode::Editing {
            self.unscaled_dt = dt;
            self.unscaled_time += f64::from(dt);
            self.sim_tick = self.sim_tick.wrapping_add(1);
        }
        // A joined world runs no game of its own: it sends input and shows the host's.
        let mut role = self.net.take();
        if let Some(net::NetRole::Join(join)) = role.as_mut() {
            let playing = self.mode == PlayMode::Playing;
            if playing {
                join.frame(self, dt);
                self.play_time += dt;
            }
            self.net = role;
            return playing;
        }
        if let Some(net::NetRole::Host(host)) = role.as_mut() {
            if self.mode == PlayMode::Playing {
                host.read_input(self);
            }
        }
        if self.mode == PlayMode::Playing {
            self.exchange_controls(dt * self.time_scale.max(0.0));
            self.exchange_saves();
            self.exchange_cloud();
            self.exchange_scenes();
        }
        let stepped = self.step_inner(dt);
        if self.mode == PlayMode::Playing {
            let motion = self.motion_dt(dt);
            self.physics.step_linked(motion, &mut self.entities);
            self.apply_volumes(motion);
        }
        if let Some(net::NetRole::Host(host)) = role.as_mut() {
            if self.mode == PlayMode::Playing {
                host.send_snapshot(self);
            }
        }
        if self.net.is_none() {
            self.net = role;
        }
        if self.at_breakpoint().is_some() {
            if self.mode != PlayMode::Paused {
                self.play_music.push(scripts::MusicCue::Pause);
            }
            self.mode = PlayMode::Paused;
        }
        if self.mode == PlayMode::Playing {
            self.step_game_screens(dt);
        }
        if stepped {
            if let Some(scripts) = self.scripts.as_mut() {
                scripts.follow(&mut self.entities);
                self.play_sounds.append(&mut scripts.sounds);
                self.play_music.append(&mut scripts.music);
            }
        }
        self.place_sockets();
        if self.mode == PlayMode::Playing {
            remote_transform::step(&mut self.entities);
        }
        // The camera moves whenever the game runs, even in a scene with nothing to simulate.
        if self.mode != PlayMode::Editing && (stepped || self.mode == PlayMode::Playing) {
            self.step_camera(self.motion_dt(dt));
        }
        let quit = self.play_game().is_some_and(|g| g.state == "quit");
        if quit {
            self.quit = true;
            self.stop();
        }
        stepped && !quit
    }

    /// Seconds of motion this frame. A script that sets the game state to
    /// `paused` freezes the world and keeps menus and scripts running.
    /// `quit` is handled by the caller, which leaves play.
    fn script_phase(&mut self, name: &str, dt: f32) {
        if let Some(scripts) = self.scripts.as_mut() {
            scripts.phase(name, dt, &mut self.entities, &mut self.play_log);
        }
    }

    /// `fixed_tick` for one update group, in that group's order.
    fn script_phase_group(&mut self, name: &str, group: &str, dt: f32) {
        if let Some(scripts) = self.scripts.as_mut() {
            scripts.phase_group(name, group, dt, &mut self.entities, &mut self.play_log);
        }
    }

    /// Frame tick for one update group.
    fn script_frame_group(&mut self, group: &str, dt: f32) {
        if let Some(scripts) = self.scripts.as_mut() {
            scripts.step_group(group, dt, &mut self.entities, &mut self.play_log);
        }
    }

    /// After-physics and late groups, then `late_tick` for every graph that listens.
    fn script_after_physics(&mut self, dt: f32) {
        self.script_frame_group(script_groups::AFTER, dt);
        self.script_phase_group("fixed_tick", script_groups::AFTER, dt);
        self.script_frame_group(script_groups::LATE, dt);
        self.script_phase_group("fixed_tick", script_groups::LATE, dt);
        self.script_phase("late_tick", dt);
    }

    fn apply_volumes(&mut self, dt: f32) {
        let outcome = if let Some(scripts) = self.scripts.as_mut() {
            volumes::step(&mut self.entities, &mut scripts.game, &mut self.volume_memory, dt)
        } else {
            volumes::step(&mut self.entities, &mut self.rules, &mut self.volume_memory, dt)
        };
        for cue in &outcome.cues {
            self.play_log.push(format!("{} {} {}", cue.event, cue.volume, cue.other));
        }
        let cues = outcome.cues;
        let remove = outcome.remove;
        if let Some(scripts) = self.scripts.as_mut() {
            for cue in &cues {
                scripts.emit_on(
                    &cue.volume,
                    &cue.event,
                    &cue.other,
                    &mut self.entities,
                    &mut self.play_log,
                );
            }
            if !remove.is_empty() {
                scripts.drop_ids(&remove, &mut self.entities, &mut self.play_log);
            }
        } else if !remove.is_empty() {
            self.entities.retain(|e| !remove.iter().any(|id| id == &e.id));
        }
    }

    fn motion_dt(&self, dt: f32) -> f32 {
        let scale = if self.scripts.as_ref().is_some_and(|s| s.game.state == "paused") {
            0.0
        } else {
            self.time_scale.max(0.0)
        };
        dt * scale
    }

    /// Put objects that name a parent socket on that point. Editing leaves them where they were saved.
    fn place_sockets(&mut self) {
        if self.mode == PlayMode::Editing {
            return;
        }
        sprite_socket::apply(&mut self.entities, self.project_dir.as_deref(), self.play_time);
    }

    /// Show the screens scripts asked for, copy their texts in, and turn
    /// clicks and changes into `ui_<Name>_clicked` / `ui_<Name>_changed` events.
    /// A typewriter that finishes fires `typewriter_<Name>_finished`.
    fn step_game_screens(&mut self, dt: f32) {
        let cmds =
            self.scripts.as_mut().map(|s| std::mem::take(&mut s.ui.typewriter)).unwrap_or_default();
        for (element, skip) in cmds {
            self.game_screens.typewriter(&element, skip);
        }
        let dir = self.project_dir.clone();
        let ask = self.scripts.as_ref().and_then(|s| s.ui.dialog.clone());
        if let Some(scripts) = self.scripts.as_mut() {
            if ask.is_some() {
                scripts.ui.shown.insert(dialog_ui::WIDGET.to_string());
            } else {
                scripts.ui.shown.remove(dialog_ui::WIDGET);
            }
        }
        if let Some(ask) = &ask {
            if let Err(err) = dialog_ui::install(&mut self.game_screens, ask, dir.as_deref()) {
                self.play_log.push(err);
            }
        } else {
            dialog_ui::dismiss(&mut self.game_screens);
        }
        if let Some((shown, texts)) =
            self.scripts.as_ref().map(|s| (s.ui.shown.clone(), s.ui.texts.clone()))
        {
            self.game_screens.sync(dir.as_deref(), &shown, &texts);
            for e in std::mem::take(&mut self.game_screens.errors) {
                self.play_log.push(e);
            }
        }
        let finished = self.game_screens.advance(self.motion_dt(dt));
        for warning in self.game_screens.take_warnings() {
            self.play_log.push(warning);
        }
        if let Some(scripts) = self.scripts.as_mut() {
            for name in finished {
                let event = format!("typewriter_{name}_finished");
                scripts.fire(&event, &mut self.entities, &mut self.play_log);
            }
        }
        self.feed_pad_focus();
        let events = self.game_screens.take_events();
        let mut answer = None;
        if ask.is_some() {
            for event in &events {
                if let Some(button) = dialog_ui::clicked(event) {
                    answer = Some((button.to_string(), dialog_ui::answer_text(&self.game_screens)));
                }
            }
        }
        if let Some(scripts) = self.scripts.as_mut() {
            for event in events {
                if answer.is_some() && dialog_ui::clicked(&event).is_some() {
                    continue;
                }
                scripts.ui.values = self.game_screens.values();
                scripts.fire(&event.script_event(), &mut self.entities, &mut self.play_log);
            }
        }
        if let Some((button, text)) = answer {
            if let Some(scripts) = self.scripts.as_mut() {
                scripts.ui.dialog = None;
                scripts.ui.shown.remove(dialog_ui::WIDGET);
                scripts.ui.dialog_button = button;
                scripts.ui.dialog_text = text;
                scripts.fire("dialog_answered", &mut self.entities, &mut self.play_log);
            }
            dialog_ui::dismiss(&mut self.game_screens);
        }
        if let Some(scripts) = self.scripts.as_mut() {
            scripts.ui.values = self.game_screens.values();
        }
    }

    /// Paint list of the screens shown in play, in pixels of the play view.
    pub fn play_screen_draws(&self) -> Vec<gameui::Draw> {
        if self.mode == PlayMode::Editing {
            return Vec::new();
        }
        self.game_screens.draws(self.play_time)
    }

    /// The first visible object with its 2D camera turned on.
    pub fn camera2d_object(&self) -> Option<&Entity> {
        self.entities.iter().find(|e| e.camera2d.on && !e.hidden)
    }

    /// Where the 2D camera wants to look: the object it follows (by name, then
    /// id), else the player, else the camera object itself.
    fn camera2d_target(&self, cam: &Entity) -> [f32; 2] {
        let spectate = self
            .play_game()
            .and_then(|g| g.spectate.clone())
            .or_else(|| self.rules.spectate.clone());
        if let Some(id) = spectate.as_deref() {
            if let Some(found) =
                self.entities.iter().find(|e| !e.hidden && (e.id == id || e.name == id))
            {
                return [found.translation[0], found.translation[1]];
            }
        }
        let follow = cam.camera2d.follow.trim();
        let found = if follow.is_empty() {
            self.play_player()
                .and_then(|id| self.entity(id))
                .or_else(|| self.entities.iter().find(|e| e.tag == "player" && !e.hidden))
        } else {
            self.entities
                .iter()
                .find(|e| e.name == follow)
                .or_else(|| self.entities.iter().find(|e| e.id == follow))
        };
        let at = found.unwrap_or(cam).translation;
        [at[0], at[1]]
    }

    /// Shake the play view: up to `strength` units, fading over `seconds`.
    pub fn camera_shake(&mut self, strength: f32, seconds: f32) {
        self.camera_rig.shake(strength, seconds);
    }

    fn step_camera(&mut self, dt: f32) {
        if let Some(scripts) = self.scripts.as_mut() {
            for (strength, seconds) in std::mem::take(&mut scripts.ui.shakes) {
                self.camera_rig.shake(strength, seconds);
            }
        }
        match self.camera2d_object() {
            Some(cam) => {
                let (target, settings) = (self.camera2d_target(cam), cam.camera2d.clone());
                self.camera_rig.step_in_view(target, &settings, self.camera_view_half, dt);
            }
            None => self.camera_rig.step_shakes(dt),
        }
    }

    /// Tell the 2D camera how large the view is, so drag margins are fractions
    /// of it. `half` is half the width and half the height, in scene units.
    pub fn set_camera_view_half(&mut self, half: [f32; 2]) {
        if half[0].is_finite() && half[1].is_finite() {
            self.camera_view_half = [half[0].max(0.0), half[1].max(0.0)];
        }
    }

    fn step_anim_events(&mut self) {
        let cues = anim_events::collect(
            &self.entities,
            self.project_dir.as_deref(),
            self.play_time,
            &mut self.anim_cursor,
        );
        if cues.is_empty() {
            return;
        }
        let Some(scripts) = self.scripts.as_mut() else { return };
        for cue in cues {
            scripts.emit_text(
                &cue.entity,
                cue.event,
                &cue.text,
                &mut self.entities,
                &mut self.play_log,
            );
        }
    }

    /// The 2D camera's view while the world runs; `None` while editing or when no
    /// object has its 2D camera on.
    pub fn play_camera2d(&self) -> Option<camera2d::View> {
        if self.mode == PlayMode::Editing {
            return None;
        }
        self.camera_rig.view(&self.camera2d_object()?.camera2d)
    }

    /// How far running shakes push the play view, in scene units.
    pub fn play_shake(&self) -> [f32; 2] {
        if self.mode == PlayMode::Editing {
            return [0.0; 2];
        }
        self.camera_rig.shake_offset()
    }

    /// Start (or restart) a keyframe animation on an object during play.
    pub fn play_anim(&mut self, id: &str, anim: seal_scene::anim::Anim) {
        self.anims.retain(|a| a.0 != id);
        self.anims.push((id.to_string(), anim, self.play_time));
    }

    pub fn stop_anim(&mut self, id: &str) {
        self.anims.retain(|a| a.0 != id);
        if let Some(e) = self.entities.iter_mut().find(|e| e.id == id) {
            e.frame_override = None;
        }
    }

    /// Start (or restart) a sequence during play; events at time 0 fire on the first step.
    pub fn play_sequence(&mut self, seq: seal_scene::sequence::Sequence) {
        self.sequences.push((seq, self.play_time, -1.0));
    }

    /// Put every object a sequence drives where it is at time `t` (editor scrubbing; fires no events).
    pub fn scrub_sequence(&mut self, seq: &seal_scene::sequence::Sequence, t: f32) {
        for (id, prop, v) in seq.sample(t) {
            if let Some(e) = self.entities.iter_mut().find(|e| e.id == id || e.name == id) {
                Self::apply_prop(e, prop, v);
            }
        }
        self.apply_sequence_properties(seq, t);
    }

    fn apply_prop(e: &mut Entity, prop: &str, v: f32) {
        if prop == "visible" {
            e.hidden = v < 0.5;
            return;
        }
        if prop == "sprite_frame" {
            e.frame_override = Some(v.max(0.0).round() as u32);
            return;
        }
        let (group, axis) = prop.split_once('.').unwrap_or((prop, ""));
        let i = match axis {
            "x" | "r" => 0,
            "y" | "g" => 1,
            "z" | "b" => 2,
            _ => return,
        };
        match group {
            "position" => e.translation[i] = v,
            "rotation" => e.rotation[i] = v,
            "scale" => e.scale[i] = v,
            "color" => e.color[i] = v.clamp(0.0, 1.0),
            _ => {}
        }
    }

    /// Attach a particle emitter to an object for this play (replaces one already on it).
    pub fn play_emitter(&mut self, id: &str, def: particles::EmitterDef) {
        self.emitters.retain(|e| e.0 != id);
        self.emitters.push((id.to_string(), particles::Emitter::new(def)));
    }

    /// Start the particle effect of every object that names one.
    fn start_scene_emitters(
        &mut self,
        dir: &std::path::Path,
        only: Option<&std::collections::BTreeSet<String>>,
    ) {
        let wanted: Vec<(String, String)> = self
            .entities
            .iter()
            .filter(|e| !e.particles.trim().is_empty())
            .filter(|e| only.is_none_or(|ids| ids.contains(&e.id)))
            .map(|e| (e.id.clone(), e.particles.trim().to_string()))
            .collect();
        for (id, path) in wanted {
            match std::fs::read_to_string(dir.join(&path))
                .map_err(|e| e.to_string())
                .and_then(|t| particles::EmitterDef::parse(&t))
            {
                Ok(def) => self.play_emitter(&id, def),
                Err(e) => self.play_log.push(format!("particles {path}: {e}")),
            }
        }
    }

    fn step_emitters(&mut self, dt: f32) {
        for (id, em) in &mut self.emitters {
            let origin = self
                .entities
                .iter()
                .find(|e| e.id == *id)
                .map(|e| e.translation)
                .unwrap_or([0.0; 3]);
            em.step(dt, origin);
        }
    }

    fn apply_sequences(&mut self) {
        let now = self.play_time;
        for (seq, start, last) in std::mem::take(&mut self.sequences) {
            let t = now - start;
            for (id, prop, v) in seq.sample(t) {
                if let Some(e) = self.entities.iter_mut().find(|e| e.id == id || e.name == id) {
                    Self::apply_prop(e, prop, v);
                }
            }
            self.apply_sequence_properties(&seq, t);
            for name in seq.events_between(last, t) {
                self.play_log.push(format!("event {name}"));
                self.sequence_events.push(name.to_string());
            }
            if !seq.finished(t) {
                self.sequences.push((seq, start, t));
            }
        }
    }

    fn run_anim_cmds(&mut self) {
        let cmds =
            self.scripts.as_mut().map(|s| std::mem::take(&mut s.anim_cmds)).unwrap_or_default();
        for (id, path) in cmds {
            let Some(path) = path else {
                self.stop_anim(&id);
                continue;
            };
            if let Some(clip) = path.strip_prefix("clip:") {
                let now = self.play_time;
                match self.entities.iter_mut().find(|e| e.id == id) {
                    Some(e) => e.play_animation(clip, now),
                    None => self.play_log.push(format!("play_clip: no object {id}")),
                }
                continue;
            }
            let Some(dir) = self.project_dir.clone() else { continue };
            if id.is_empty() {
                match std::fs::read_to_string(dir.join(&path))
                    .map_err(|e| e.to_string())
                    .and_then(|t| seal_scene::sequence::Sequence::parse(&t))
                {
                    Ok(s) => self.play_sequence(s),
                    Err(e) => self.play_log.push(format!("sequence {path}: {e}")),
                }
                continue;
            }
            match std::fs::read_to_string(dir.join(&path))
                .map_err(|e| e.to_string())
                .and_then(|t| seal_scene::anim::Anim::parse(&t))
            {
                Ok(a) => self.play_anim(&id, a),
                Err(e) => self.play_log.push(format!("animation {path}: {e}")),
            }
        }
    }

    fn apply_anims(&mut self) {
        let now = self.play_time;
        for (id, anim, start) in std::mem::take(&mut self.anims) {
            let t = now - start;
            if let Some(e) = self.entities.iter_mut().find(|e| e.id == id) {
                for (target, v) in anim.sample(t) {
                    let (group, axis) = target.split_once('.').unwrap_or((target, ""));
                    let i = match axis {
                        "x" | "r" => 0,
                        "y" | "g" => 1,
                        "z" | "b" => 2,
                        _ => continue,
                    };
                    match group {
                        "position" => e.translation[i] = v,
                        "rotation" => e.rotation[i] = v,
                        "scale" => e.scale[i] = v,
                        "color" => e.color[i] = v.clamp(0.0, 1.0),
                        _ => {}
                    }
                }
                if let Some((_, v)) = anim.sample(t).into_iter().find(|(t, _)| *t == "sprite_frame")
                {
                    e.frame_override = Some(v.max(0.0).round() as u32);
                }
                if anim.finished(t) {
                    e.frame_override = None;
                }
                if !anim.finished(t) {
                    self.anims.push((id, anim, start));
                }
            }
        }
    }

    fn publish_areas(&mut self) {
        let report = if let Some(sim) = &self.sim2d {
            sim.area_report()
        } else if let Some(game) = &self.platformer {
            game.area_report()
        } else {
            return;
        };
        if let Some(scripts) = self.scripts.as_mut() {
            scripts.areas(&report, &mut self.entities, &mut self.play_log);
        }
    }

    /// Apply a script's `play.time_scale` and return this frame's scaled step.
    fn scaled_dt(&mut self, dt: f32) -> f32 {
        if let Some(scale) = self.scripts.as_mut().and_then(|s| s.ui.time_scale.take()) {
            self.time_scale = scale.max(0.0);
        }
        self.motion_dt(dt)
    }

    /// Newly pressed pad buttons move and activate on-screen focus.
    fn feed_pad_focus(&mut self) {
        if !self.game_screens.active() {
            self.controls.ui_pads.clear();
            return;
        }
        let down: Vec<String> = self
            .controls
            .raw
            .buttons()
            .filter(|b| b.name().starts_with("pad."))
            .map(|b| b.name().to_string())
            .collect();
        let fresh: Vec<String> = down
            .iter()
            .filter(|b| !self.controls.ui_pads.iter().any(|h| h == *b))
            .cloned()
            .collect();
        self.controls.ui_pads = down.into_iter().collect();
        for name in fresh {
            let key = match name.as_str() {
                "pad.dpad_up" => "DPadUp",
                "pad.dpad_down" => "DPadDown",
                "pad.dpad_left" => "DPadLeft",
                "pad.dpad_right" => "DPadRight",
                "pad.south" => "GamepadSouth",
                "pad.east" => "GamepadEast",
                _ => continue,
            };
            self.game_screens.key(key);
        }
    }

    fn step_inner(&mut self, dt: f32) -> bool {
        if self.mode != PlayMode::Playing {
            return false;
        }
        let mut motion = self.scaled_dt(dt);
        self.play_time += motion;
        self.run_anim_cmds();
        self.step_anim_events();
        self.apply_anims();
        self.apply_sequences();
        self.step_emitters(motion);
        self.walkers.step(motion, &mut self.entities);
        self.followers.step(motion, &mut self.entities);
        let char_views = self.characters.views();
        if let Some(scripts) = self.scripts.as_mut() {
            scripts.set_play_clock(self.unscaled_time, self.unscaled_dt, self.sim_tick);
            scripts.char_view = char_views;
            for (key, pressed) in std::mem::take(&mut self.play_keys) {
                scripts.key(&key, pressed, &mut self.entities, &mut self.play_log);
            }
            scripts.step(motion, &mut self.entities, &mut self.play_log);
        }
        motion = self.scaled_dt(dt);
        if let Some(scripts) = self.scripts.as_mut() {
            for (id, goal) in std::mem::take(&mut scripts.ai_cmds) {
                self.ai.order(&id, goal);
            }
            self.ai.step(motion, &mut self.entities, &mut self.play_log);
            scripts.ai_arrived = self.ai.arrived.clone();
        }
        if let Some(cmds) = self.scripts.as_mut().map(|s| std::mem::take(&mut s.char_cmds)) {
            for cmd in cmds {
                self.characters.command(cmd);
            }
        }
        // A script may have paused during this step; the body should not move after that.
        let motion = self.motion_dt(dt);
        self.characters.step(motion, &mut self.entities);
        if self.scripts.is_some() {
            if self.sim2d.is_some() {
                self.script_phase_group("fixed_tick", script_groups::BEFORE, motion);
                if let Some(sim2d) = self.sim2d.as_mut() {
                    if let Some(scripts) = self.scripts.as_mut() {
                        let pending = std::mem::take(&mut scripts.phys2d);
                        sim2d.apply_pending(&pending);
                    }
                    sim2d.step(motion, &mut self.entities);
                    let breaks = sim2d.joint_break_scene_ids();
                    if !breaks.is_empty() {
                        if let Some(scripts) = self.scripts.as_mut() {
                            scripts.joint_breaks(&breaks, &mut self.entities, &mut self.play_log);
                        }
                        sim2d.clear_broken_joints(&mut self.entities);
                    }
                    let report = sim2d.area_report();
                    if let Some(scripts) = self.scripts.as_mut() {
                        scripts.areas(&report, &mut self.entities, &mut self.play_log);
                    }
                }
                self.script_after_physics(motion);
                return true;
            }
            if self.platformer.is_none() {
                self.script_phase_group("fixed_tick", script_groups::BEFORE, motion);
                #[cfg(feature = "play-preview")]
                self.drive_characters();
                #[cfg(feature = "play-preview")]
                if let Some(sim) = self.sim.as_mut() {
                    sim.step(motion, self.gravity, &mut self.entities);
                }
                self.script_after_physics(motion);
                return true;
            }
        }
        if self.platformer.is_some() {
            self.script_phase_group("fixed_tick", script_groups::BEFORE, motion);
            if let Some(game) = self.platformer.as_mut() {
                game.step(motion, self.play_input, &mut self.entities);
                let breaks = game.joint_break_scene_ids();
                if !breaks.is_empty() {
                    if let Some(scripts) = self.scripts.as_mut() {
                        scripts.joint_breaks(&breaks, &mut self.entities, &mut self.play_log);
                    }
                    game.clear_broken_joints(&mut self.entities);
                }
            }
            self.publish_areas();
            self.script_after_physics(motion);
            return true;
        }
        if self.sim2d.is_some() {
            self.script_phase_group("fixed_tick", script_groups::BEFORE, motion);
            if let Some(sim2d) = self.sim2d.as_mut() {
                sim2d.step(motion, &mut self.entities);
                let breaks = sim2d.joint_break_scene_ids();
                if !breaks.is_empty() {
                    if let Some(scripts) = self.scripts.as_mut() {
                        scripts.joint_breaks(&breaks, &mut self.entities, &mut self.play_log);
                    }
                    sim2d.clear_broken_joints(&mut self.entities);
                }
            }
            self.publish_areas();
            self.script_after_physics(motion);
            return true;
        }
        #[cfg(feature = "play-preview")]
        {
            if self.sim.is_none() {
                return false;
            }
            self.script_phase_group("fixed_tick", script_groups::BEFORE, motion);
            if let Some(sim) = self.sim.as_mut() {
                sim.step(motion, self.gravity, &mut self.entities);
            }
            self.script_after_physics(motion);
            true
        }
        #[cfg(not(feature = "play-preview"))]
        {
            let _ = dt;
            false
        }
    }

    /// The scene in the old flat format (still readable by `from_text`).
    pub fn to_legacy_text(&self) -> String {
        let entities = SealValue::Array(self.entities.iter().map(entity_value).collect());
        SealValue::Map(
            vec![
                ("schema_version".into(), SealValue::Number("1".into())),
                ("gravity".into(), number(self.gravity)),
                ("time_scale".into(), number(self.time_scale)),
                ("music".into(), SealValue::String(self.music.clone())),
                ("entities".into(), entities),
            ]
            .into(),
        )
        .to_text()
    }

    pub fn from_text(text: &str) -> Result<Self, String> {
        let value = parse(text).map_err(|err| err.to_string())?;
        if value.get("seal_scene").is_some() {
            return Self::from_scene_text(text);
        }
        let Some(items) = value.get("entities").and_then(|value| match value {
            SealValue::Array(items) => Some(items),
            _ => None,
        }) else {
            return Err("scene is missing entities".into());
        };
        let mut entities = Vec::new();
        for item in items {
            entities.push(entity_from(item)?);
        }
        if entities.is_empty() {
            return Err("scene has no entities".into());
        }
        let time_scale = match value.get("time_scale") {
            Some(SealValue::Number(text)) => text.parse().unwrap_or(1.0),
            _ => 1.0,
        };
        let gravity = match value.get("gravity") {
            Some(SealValue::Number(text)) => text.parse().unwrap_or(DEFAULT_GRAVITY),
            _ => DEFAULT_GRAVITY,
        };
        let music = match value.get("music") {
            Some(SealValue::String(text)) => text.clone(),
            _ => String::new(),
        };
        let mut world = Self::with_entities(entities, gravity, time_scale);
        world.music = music;
        Ok(world)
    }

    /// A new scene file holding only a root object called `name`.
    pub fn empty_scene_text(name: &str) -> String {
        let root = entity("root", name, None, [0.0, 0.0, 0.0], None, false, "");
        Self::with_entities(vec![root], DEFAULT_GRAVITY, 1.0).to_text()
    }

    pub(crate) fn with_entities(entities: Vec<Entity>, gravity: f32, time_scale: f32) -> Self {
        Self {
            entities,
            mode: PlayMode::Editing,
            gravity,
            time_scale,
            phys2d_settings: seal_physics2d::Physics2DSettings::default(),
            snapshot: None,
            #[cfg(feature = "play-preview")]
            sim: None,
            platformer: None,
            sim2d: None,
            project_dir: None,
            play_keys: Vec::new(),
            game_screens: gameui::Screens::default(),
            play_sounds: Vec::new(),
            play_music: Vec::new(),
            music: String::new(),
            sky_cubemap: String::new(),
            glow: seal_scene::Glow2d::default(),
            breakpoints: Default::default(),
            play_log: Vec::new(),
            play_time: 0.0,
            unscaled_time: 0.0,
            unscaled_dt: 0.0,
            sim_tick: 0,
            anims: Vec::new(),
            sequences: Vec::new(),
            sequence_warned: std::collections::BTreeSet::new(),
            emitters: Vec::new(),
            net: None,
            sequence_events: Vec::new(),
            ai: ai::Ai::default(),
            walkers: nav2d::Walkers::default(),
            characters: character2d::Motion::default(),
            followers: shape2d::Followers::default(),
            camera_rig: camera2d::Rig::default(),
            camera_view_half: [0.0, 0.0],
            look_camera: false,
            anim_cursor: std::collections::BTreeMap::new(),
            controls: controls::Controls::default(),
            scenes: scenes::Scenes::default(),
            quit: false,
            join_on: false,
            volume_memory: volumes::Memory::default(),
            join_roster: players::Roster::default(),
            rules: game::Game::default(),
            objects_per_file: false,
            physics: physics::PhysicsState::default(),
            scripts: None,
            play_input: PlayInput::default(),
        }
    }
}

fn entity(
    id: &str,
    name: &str,
    parent: Option<&str>,
    translation: [f32; 3],
    shape: Option<Shape>,
    dynamic: bool,
    tag: &str,
) -> Entity {
    Entity {
        id: id.into(),
        name: name.into(),
        parent: parent.map(str::to_string),
        translation,
        shape,
        dynamic,
        friction: 0.6,
        restitution: 0.05,
        walk_speed: 0.0,
        velocity: [0.0, 0.0, 0.0],
        tag: tag.into(),
        rotation: [0.0, 0.0, 0.0],
        scale: [1.0, 1.0, 1.0],
        color: color_for(tag),
        hidden: false,
        locked: false,
        unselectable: false,
        roughness: 0.5,
        metal: 0.0,
        intensity: 1.0,
        fov: 60.0,
        near: 0.1,
        far: 1000.0,
        source_kind: String::new(),
        source_asset: String::new(),
        overrides: String::new(),
        outliner_unsaved: false,
        outliner_play_spawned: false,
        outliner_warning_key: String::new(),
        sprite: String::new(),
        layer: 0,
        frames: 1,
        fps: 8.0,
        frame_override: None,
        animation: String::new(),
        anim_start: 0.0,
        flip: [false, false],
        tint: [1.0; 4],
        order: 0,
        slice: [0.0; 4],
        slice_mode: "stretch".into(),
        slice_size: [0.0; 2],
        pivot: [0.5, 0.5],
        sort_mode: sprite_sort::ObjectSortMode::Default,
        normal_map: String::new(),
        parallax: [1.0; 2],
        repeat_x: false,
        light2d: seal_scene::Light2d::default(),
        camera2d: seal_scene::Camera2d::default(),
        audio_listener: seal_scene::AudioListener::default(),
        phys2d: seal_scene::Physics2d::default(),
        area: seal_physics2d::AreaEffect::default(),
        surface: seal_physics2d::SurfaceEffect::default(),
        nav2d: seal_scene::NavAgent2d::default(),
        character2d: seal_scene::Character2d::default(),
        shape2d: seal_scene::Shape2d::default(),
        sprite_shape: seal_scene::SpriteShape::default(),
        follow: seal_scene::PathFollow::default(),
        material2d: seal_scene::Material2d::default(),
        mask: String::new(),
        sprite_mask: seal_scene::SpriteMask::default(),
        script: String::new(),
        particles: String::new(),
        socket: String::new(),
        volume: volumes::Volume::default(),
        script_order: 0,
        script_group: String::new(),
        remote: remote_transform::RemoteTransform::default(),
        collider_enabled: true,
        merged: object_merge::MergedDraw::default(),
        post: post_volume::PostVolume::default(),
        mesh_material: seal_scene::MeshMaterial::default(),
    }
}

fn tint_entity(entities: &mut [Entity], id: &str, color: [f32; 3]) {
    if let Some(entity) = entities.iter_mut().find(|entity| entity.id == id) {
        entity.color = color;
    }
}

fn color_for(tag: &str) -> [f32; 3] {
    match tag {
        "prop" => [0.86, 0.52, 0.24],
        "camera" => [0.32, 0.55, 0.92],
        "light" => [0.95, 0.82, 0.28],
        "ground" => [0.42, 0.46, 0.5],
        "player" => [0.35, 0.72, 0.55],
        _ => [0.72, 0.74, 0.78],
    }
}

fn entity_value(entity: &Entity) -> SealValue {
    let mut extra: Vec<(String, SealValue)> = Vec::new();
    for (k, v) in [
        ("sprite", &entity.sprite),
        ("script", &entity.script),
        ("particles", &entity.particles),
        ("socket", &entity.socket),
        ("normal_map", &entity.normal_map),
    ] {
        if !v.is_empty() {
            extra.push((k.into(), SealValue::String(v.clone())));
        }
    }
    if entity.layer != 0 {
        extra.push(("layer".into(), number(entity.layer as f32)));
    }
    if entity.frames != 1 {
        extra.push(("frames".into(), number(entity.frames as f32)));
    }
    if entity.fps != 8.0 {
        extra.push(("fps".into(), number(entity.fps)));
    }
    if !entity.animation.is_empty() {
        extra.push(("animation".into(), SealValue::String(entity.animation.clone())));
    }
    if entity.tint != [1.0; 4] {
        extra.push(("tint".into(), SealValue::String(tint_hex(&entity.tint))));
    }
    if entity.order != 0 {
        extra.push(("order".into(), number(entity.order as f32)));
    }
    if entity.slice != [0.0; 4] {
        extra.push(("slice".into(), SealValue::String(slice_text(&entity.slice))));
    }
    if entity.slice_mode != "stretch" {
        extra.push(("slice_mode".into(), SealValue::String(entity.slice_mode.clone())));
    }
    if entity.slice_size != [0.0; 2] {
        extra.push((
            "slice_size".into(),
            SealValue::String(sprite_slice::slice_size_text(entity.slice_size)),
        ));
    }
    if entity.pivot != [0.5, 0.5] {
        extra.push(("pivot".into(), SealValue::String(sprite_pivot::pivot_text(entity.pivot))));
    }
    if entity.sort_mode != sprite_sort::ObjectSortMode::Default {
        extra.push(("sort_mode".into(), SealValue::String(entity.sort_mode.name().into())));
    }
    if entity.parallax != [1.0; 2] {
        extra.push(("parallax".into(), SealValue::String(parallax_text(&entity.parallax))));
    }
    if entity.repeat_x {
        extra.push(("repeat_x".into(), SealValue::Bool(true)));
    }
    if !entity.light2d.kind.is_empty() {
        extra.push(("light2d".into(), SealValue::String(entity.light2d.kind.clone())));
        extra.push(("light_range".into(), number(entity.light2d.range)));
        extra.push(("light_cone".into(), number(entity.light2d.cone)));
        if !entity.light2d.shadows {
            extra.push(("light_shadows".into(), SealValue::Bool(false)));
        }
        for (key, value) in light_fields::rows(entity) {
            extra.push((key, SealValue::String(value)));
        }
    }
    if entity.audio_listener.on
        || (entity.audio_listener.max_distance - 50.0).abs() > 0.001
        || (entity.audio_listener.rolloff - 1.0).abs() > 0.001
    {
        extra.push(("audio_listener".into(), SealValue::Bool(entity.audio_listener.on)));
        extra.push(("listener_max_distance".into(), number(entity.audio_listener.max_distance)));
        extra.push(("listener_rolloff".into(), number(entity.audio_listener.rolloff)));
    }
    if entity.camera2d != seal_scene::Camera2d::default() {
        let c = &entity.camera2d;
        extra.push(("camera2d".into(), SealValue::Bool(c.on)));
        extra.push(("cam_follow".into(), SealValue::String(c.follow.clone())));
        extra.push(("cam_smoothing".into(), number(c.smoothing)));
        extra.push(("cam_zoom".into(), number(c.zoom)));
        extra.push(("cam_limits".into(), SealValue::String(slice_text(&c.limits))));
        extra.push(("cam_pixels".into(), number(c.pixel_snap)));
        if let Some(text) = cam_drag::save_text(c.drag) {
            extra.push(("cam_drag".into(), SealValue::String(text)));
        }
    }
    if !entity.phys2d.body.is_empty() || entity.phys2d.part {
        let mut p = entity.phys2d.clone();
        if p.joint.is_empty() {
            // Joint settings without a joint are not worth keeping.
            let d = seal_scene::Physics2d::default();
            (p.joint_to, p.anchor, p.other_anchor, p.axis, p.limits) =
                (d.joint_to, d.anchor, d.other_anchor, d.axis, d.limits);
            (p.length, p.stiffness, p.damping, p.motor) =
                (d.length, d.stiffness, d.damping, d.motor);
            (p.break_force, p.break_torque, p.max_force, p.max_torque) =
                (d.break_force, d.break_torque, d.max_force, d.max_torque);
            (p.correction, p.offset, p.target) = (d.correction, d.offset, d.target);
        }
        for key in phys2d::KEYS {
            let text = phys2d::field_text(&p, key).unwrap_or_default();
            if phys2d::default_text(key).as_deref() != Some(text.as_str()) {
                extra.push(((*key).into(), SealValue::String(text)));
            }
        }
    }
    if entity.sprite_shape.on {
        for key in sprite_shape::KEYS {
            let text = sprite_shape::field_text(entity, key).unwrap_or_default();
            extra.push(((*key).into(), SealValue::String(text)));
        }
    }
    if entity.shape2d != seal_scene::Shape2d::default()
        || entity.follow != seal_scene::PathFollow::default()
    {
        for key in shape2d::KEYS {
            let text = shape2d::field_text(entity, key).unwrap_or_default();
            extra.push(((*key).into(), SealValue::String(text)));
        }
        for key in shape2d::EXTRA {
            let text = shape2d::field_text(entity, key).unwrap_or_default();
            if text != shape2d::default_text(key) {
                extra.push(((*key).into(), SealValue::String(text)));
            }
        }
    }
    if entity.material2d != seal_scene::Material2d::default() {
        for key in material2d::KEYS {
            let text = material2d::field_text(entity, key).unwrap_or_default();
            extra.push(((*key).into(), SealValue::String(text)));
        }
    }
    if entity.area != seal_physics2d::AreaEffect::default() || entity.surface.speed != 0.0 {
        for key in area_effect::KEYS {
            let text =
                area_effect::field_text(&entity.area, &entity.surface, key).unwrap_or_default();
            let default = area_effect::field_text(
                &seal_physics2d::AreaEffect::default(),
                &seal_physics2d::SurfaceEffect::default(),
                key,
            );
            if default.as_deref() != Some(text.as_str()) {
                extra.push(((*key).into(), SealValue::String(text)));
            }
        }
    }
    if !entity.sprite_mask.shape.is_empty() {
        for key in sprite_mask::KEYS {
            if *key == "mask" {
                continue;
            }
            if let Some(text) = sprite_mask::field_text(entity, key) {
                extra.push(((*key).into(), SealValue::String(text)));
            }
        }
    }
    if !entity.mask.is_empty() {
        extra.push(("mask".into(), SealValue::String(entity.mask.clone())));
    }
    if entity.nav2d != seal_scene::NavAgent2d::default() {
        for key in nav2d::KEYS {
            let text = nav2d::field_text(&entity.nav2d, key).unwrap_or_default();
            extra.push(((*key).into(), SealValue::String(text)));
        }
    }
    if entity.character2d != seal_scene::Character2d::default() {
        for key in character2d::KEYS {
            let text = character2d::field_text(&entity.character2d, key).unwrap_or_default();
            extra.push(((*key).into(), SealValue::String(text)));
        }
    }
    for (k, on) in [("flip_x", entity.flip[0]), ("flip_y", entity.flip[1])] {
        if on {
            extra.push((k.into(), SealValue::Bool(true)));
        }
    }
    if entity.unselectable {
        extra.push(("unselectable".into(), SealValue::Bool(true)));
    }
    if entity.mesh_material != seal_scene::MeshMaterial::default() {
        extra
            .push(("mesh_texture".into(), SealValue::String(entity.mesh_material.texture.clone())));
        extra.push(("mesh_layer".into(), number(entity.mesh_material.layer as f32)));
        extra.push(("mesh_slice".into(), number(entity.mesh_material.slice)));
        extra.push(("mesh_tile_u".into(), number(entity.mesh_material.tile[0])));
        extra.push(("mesh_tile_v".into(), number(entity.mesh_material.tile[1])));
    }
    if !entity.merged.kind.is_empty() {
        extra.push(("merged_kind".into(), SealValue::String(entity.merged.kind.clone())));
        extra.push(("merged_body".into(), SealValue::String(entity.merged.body.clone())));
    }
    if !entity.volume.kind.is_empty() {
        for (key, value) in volumes::fields(&entity.volume) {
            extra.push((key, SealValue::String(value)));
        }
    }
    if entity.volume.kind == "post" {
        for (key, value) in post_volume::fields(&entity.post) {
            extra.push((key, SealValue::String(value)));
        }
    }
    SealValue::Map(
        (vec![
            ("id".into(), SealValue::String(entity.id.clone())),
            ("name".into(), SealValue::String(entity.name.clone())),
            (
                "parent".into(),
                match &entity.parent {
                    Some(parent) => SealValue::String(parent.clone()),
                    None => SealValue::Null,
                },
            ),
            ("x".into(), number(entity.translation[0])),
            ("y".into(), number(entity.translation[1])),
            ("z".into(), number(entity.translation[2])),
            ("rx".into(), number(entity.rotation[0])),
            ("ry".into(), number(entity.rotation[1])),
            ("rz".into(), number(entity.rotation[2])),
            ("sx".into(), number(entity.scale[0])),
            ("sy".into(), number(entity.scale[1])),
            ("sz".into(), number(entity.scale[2])),
            ("cr".into(), number(entity.color[0])),
            ("cg".into(), number(entity.color[1])),
            ("cb".into(), number(entity.color[2])),
            ("hidden".into(), SealValue::Bool(entity.hidden)),
            ("locked".into(), SealValue::Bool(entity.locked)),
            ("roughness".into(), number(entity.roughness)),
            ("metal".into(), number(entity.metal)),
            ("intensity".into(), number(entity.intensity)),
            ("fov".into(), number(entity.fov)),
            ("near".into(), number(entity.near)),
            ("far".into(), number(entity.far)),
            (
                "shape".into(),
                SealValue::String(entity.shape.map(Shape::label).unwrap_or("").into()),
            ),
            ("dynamic".into(), SealValue::Bool(entity.dynamic)),
            ("friction".into(), number(entity.friction)),
            ("restitution".into(), number(entity.restitution)),
            ("walk_speed".into(), number(entity.walk_speed)),
            ("tag".into(), SealValue::String(entity.tag.clone())),
            ("source_kind".into(), SealValue::String(entity.source_kind.clone())),
            ("source_asset".into(), SealValue::String(entity.source_asset.clone())),
            ("overrides".into(), SealValue::String(entity.overrides.clone())),
            ("outliner_unsaved".into(), SealValue::Bool(entity.outliner_unsaved)),
            ("outliner_play_spawned".into(), SealValue::Bool(entity.outliner_play_spawned)),
            ("outliner_warning_key".into(), SealValue::String(entity.outliner_warning_key.clone())),
        ])
        .into_iter()
        .chain(extra)
        .collect::<Vec<_>>()
        .into(),
    )
}

fn entity_from(value: &SealValue) -> Result<Entity, String> {
    let map = value.as_map().ok_or("entity must be a map")?;
    let id = string_field(map, "id")?;
    let name = string_field(map, "name").unwrap_or_else(|_| id.clone());
    let parent = match field(map, "parent") {
        Some(SealValue::String(parent)) if !parent.is_empty() => Some(parent.clone()),
        _ => None,
    };
    let shape = field(map, "shape").and_then(|value| match value {
        SealValue::String(text) => Shape::parse(text),
        _ => None,
    });
    let dynamic = matches!(field(map, "dynamic"), Some(SealValue::Bool(true)));
    let tag = match field(map, "tag") {
        Some(SealValue::String(tag)) => tag.clone(),
        _ => String::new(),
    };
    let mut entity = Entity {
        id,
        name,
        parent,
        translation: [number_field(map, "x"), number_field(map, "y"), number_field(map, "z")],
        shape,
        dynamic,
        friction: number_or(map, "friction", 0.6),
        restitution: number_or(map, "restitution", 0.05),
        walk_speed: number_or(map, "walk_speed", 0.0),
        velocity: [0.0, 0.0, 0.0],
        tag: tag.clone(),
        rotation: [number_field(map, "rx"), number_field(map, "ry"), number_field(map, "rz")],
        scale: [number_or(map, "sx", 1.0), number_or(map, "sy", 1.0), number_or(map, "sz", 1.0)],
        color: [
            number_or(map, "cr", color_for(&tag)[0]),
            number_or(map, "cg", color_for(&tag)[1]),
            number_or(map, "cb", color_for(&tag)[2]),
        ],
        hidden: matches!(field(map, "hidden"), Some(SealValue::Bool(true))),
        locked: matches!(field(map, "locked"), Some(SealValue::Bool(true))),
        unselectable: matches!(field(map, "unselectable"), Some(SealValue::Bool(true))),
        roughness: number_or(map, "roughness", 0.5),
        metal: number_or(map, "metal", 0.0),
        intensity: number_or(map, "intensity", 1.0),
        fov: number_or(map, "fov", 60.0),
        near: number_or(map, "near", 0.1),
        far: number_or(map, "far", 1000.0),
        source_kind: string_field(map, "source_kind").unwrap_or_default(),
        source_asset: string_field(map, "source_asset").unwrap_or_default(),
        overrides: string_field(map, "overrides").unwrap_or_default(),
        outliner_unsaved: matches!(field(map, "outliner_unsaved"), Some(SealValue::Bool(true))),
        outliner_play_spawned: matches!(
            field(map, "outliner_play_spawned"),
            Some(SealValue::Bool(true))
        ),
        outliner_warning_key: string_field(map, "outliner_warning_key").unwrap_or_default(),
        sprite: string_field(map, "sprite").unwrap_or_default(),
        layer: number_or(map, "layer", 0.0).round() as i32,
        frames: number_or(map, "frames", 1.0).round().max(1.0) as u32,
        fps: number_or(map, "fps", 8.0),
        frame_override: None,
        animation: string_field(map, "animation").unwrap_or_default(),
        anim_start: 0.0,
        flip: [
            matches!(field(map, "flip_x"), Some(SealValue::Bool(true))),
            matches!(field(map, "flip_y"), Some(SealValue::Bool(true))),
        ],
        tint: match field(map, "tint") {
            Some(SealValue::String(hex)) => parse_tint(hex).unwrap_or([1.0; 4]),
            _ => [1.0; 4],
        },
        order: number_or(map, "order", 0.0).round() as i32,
        slice: match field(map, "slice") {
            Some(SealValue::String(text)) => parse_slice(text).unwrap_or([0.0; 4]),
            _ => [0.0; 4],
        },
        slice_mode: string_field(map, "slice_mode").unwrap_or_else(|_| "stretch".into()),
        slice_size: match field(map, "slice_size") {
            Some(SealValue::String(text)) => {
                sprite_slice::parse_slice_size(text).unwrap_or([0.0; 2])
            }
            _ => [0.0; 2],
        },
        pivot: match field(map, "pivot") {
            Some(SealValue::String(text)) => sprite_pivot::parse_pivot(text).unwrap_or([0.5, 0.5]),
            _ => [0.5, 0.5],
        },
        sort_mode: string_field(map, "sort_mode")
            .ok()
            .map(|t| sprite_sort::ObjectSortMode::parse(&t))
            .unwrap_or_default(),
        normal_map: string_field(map, "normal_map").unwrap_or_default(),
        parallax: match field(map, "parallax") {
            Some(SealValue::String(text)) => parse_parallax(text).unwrap_or([1.0; 2]),
            _ => [1.0; 2],
        },
        repeat_x: matches!(field(map, "repeat_x"), Some(SealValue::Bool(true))),
        light2d: match string_field(map, "light2d") {
            Ok(kind) => light_fields::read_saved(
                kind,
                number_or(map, "light_range", 5.0),
                number_or(map, "light_cone", 60.0),
                !matches!(field(map, "light_shadows"), Some(SealValue::Bool(false))),
                string_field(map, "light_blend").unwrap_or_default().as_str(),
                number_or(map, "light_falloff", 1.0),
                string_field(map, "light_points").unwrap_or_default().as_str(),
            ),
            _ => seal_scene::Light2d::default(),
        },
        camera2d: match field(map, "camera2d") {
            Some(SealValue::Bool(on)) => seal_scene::Camera2d {
                on: *on,
                follow: string_field(map, "cam_follow").unwrap_or_default(),
                smoothing: number_or(map, "cam_smoothing", 0.15),
                zoom: number_or(map, "cam_zoom", 1.0),
                limits: match field(map, "cam_limits") {
                    Some(SealValue::String(t)) => parse_limits(t).unwrap_or([0.0; 4]),
                    _ => [0.0; 4],
                },
                pixel_snap: number_or(map, "cam_pixels", 0.0),
                cull_mask: match field(map, "cam_cull_mask") {
                    Some(SealValue::String(text)) => phys2d::parse_layers(text).unwrap_or(u32::MAX),
                    _ => u32::MAX,
                },
                drag: match field(map, "cam_drag") {
                    Some(SealValue::String(text)) => cam_drag::from_saved(Some(text)),
                    _ => [0.0; 4],
                },
            },
            _ => seal_scene::Camera2d::default(),
        },
        audio_listener: {
            let mut listener = seal_scene::AudioListener::default();
            listener.on = matches!(field(map, "audio_listener"), Some(SealValue::Bool(true)))
                || matches!(field(map, "audio_listener"), Some(SealValue::String(text)) if text == "yes");
            listener.max_distance = number_or(map, "listener_max_distance", listener.max_distance);
            listener.rolloff = number_or(map, "listener_rolloff", listener.rolloff);
            listener
        },
        phys2d: {
            let mut p = seal_scene::Physics2d::default();
            for key in phys2d::KEYS {
                if let Some(SealValue::String(text)) = field(map, key) {
                    phys2d::apply(&mut p, key, text);
                }
            }
            p
        },
        area: seal_physics2d::AreaEffect::default(),
        surface: seal_physics2d::SurfaceEffect::default(),
        nav2d: {
            let mut n = seal_scene::NavAgent2d::default();
            for key in nav2d::KEYS {
                if let Some(SealValue::String(text)) = field(map, key) {
                    nav2d::apply(&mut n, key, text);
                }
            }
            n
        },
        character2d: seal_scene::Character2d::default(),
        shape2d: seal_scene::Shape2d::default(),
        sprite_shape: seal_scene::SpriteShape::default(),
        follow: seal_scene::PathFollow::default(),
        material2d: seal_scene::Material2d::default(),
        mask: String::new(),
        sprite_mask: seal_scene::SpriteMask::default(),
        script: string_field(map, "script").unwrap_or_default(),
        particles: string_field(map, "particles").unwrap_or_default(),
        socket: string_field(map, "socket").unwrap_or_default(),
        volume: volumes::Volume::default(),
        script_order: 0,
        script_group: String::new(),
        remote: remote_transform::RemoteTransform::default(),
        collider_enabled: true,
        merged: object_merge::MergedDraw {
            kind: string_field(map, "merged_kind").unwrap_or_default(),
            body: string_field(map, "merged_body").unwrap_or_default(),
        },
        post: post_volume::PostVolume::default(),
        mesh_material: mesh_surface::read_saved(map),
    };
    if let Some(SealValue::String(text)) = field(map, "volume_kind") {
        volumes::apply(&mut entity.volume, "volume_kind", text);
    }
    for key in volumes::KEYS {
        if let Some(SealValue::String(text)) = field(map, key) {
            volumes::apply(&mut entity.volume, key, text);
        }
    }
    for key in post_volume::KEYS {
        if let Some(SealValue::String(text)) = field(map, key) {
            post_volume::apply(&mut entity.post, key, text);
        }
    }
    for key in shape2d::KEYS.iter().chain(shape2d::EXTRA) {
        if let Some(SealValue::String(text)) = field(map, key) {
            shape2d::apply(&mut entity, key, text);
        }
    }
    let has_sprite = sprite_shape::KEYS.iter().any(|key| field(map, key).is_some());
    if has_sprite {
        if let Some(SealValue::String(text)) = field(map, "ss_points") {
            if sprite_shape::check("ss_points", text).is_some() {
                return Err("sprite shape point list is damaged".into());
            }
        }
        entity.sprite_shape.on = true;
        for key in sprite_shape::KEYS {
            if let Some(SealValue::String(text)) = field(map, key) {
                sprite_shape::apply(&mut entity, key, text);
            }
        }
    }
    for key in area_effect::KEYS {
        if let Some(SealValue::String(text)) = field(map, key) {
            area_effect::apply(&mut entity.area, &mut entity.surface, key, text);
        }
    }
    for key in character2d::KEYS {
        if let Some(SealValue::String(text)) = field(map, key) {
            if character2d::check(key, text).is_some() {
                return Err(format!("character 2d field {key} is damaged"));
            }
            character2d::apply(&mut entity.character2d, key, text);
        }
    }
    for key in material2d::KEYS {
        if let Some(SealValue::String(text)) = field(map, key) {
            material2d::apply(&mut entity, key, text);
        }
    }
    for key in sprite_mask::KEYS {
        if let Some(SealValue::String(text)) = field(map, key) {
            sprite_mask::apply(&mut entity, key, text);
        }
    }
    Ok(entity)
}

fn field<'a>(map: &'a [(String, SealValue)], key: &str) -> Option<&'a SealValue> {
    map.iter().find(|(name, _)| name == key).map(|(_, value)| value)
}

fn string_field(map: &[(String, SealValue)], key: &str) -> Result<String, String> {
    match field(map, key) {
        Some(SealValue::String(text)) if !text.is_empty() => Ok(text.clone()),
        _ => Err(format!("entity is missing {key}")),
    }
}

fn number_field(map: &[(String, SealValue)], key: &str) -> f32 {
    number_or(map, key, 0.0)
}

fn number_or(map: &[(String, SealValue)], key: &str, fallback: f32) -> f32 {
    match field(map, key) {
        Some(SealValue::Number(text)) => text.parse().unwrap_or(fallback),
        _ => fallback,
    }
}

fn number(value: f32) -> SealValue {
    SealValue::Number(if value.is_finite() { format_num(value) } else { "0".into() })
}

fn format_num(value: f32) -> String {
    let text = format!("{value:.4}");
    let trimmed = text.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() || trimmed == "-" {
        "0".into()
    } else {
        trimmed.into()
    }
}

/// Walk direction (x, z) for forward/right input. With a play camera of yaw `yaw`
/// (degrees; it looks along (sin, 0, cos), so yaw 0 is the Front view's +z and +x is
/// on its right) forward walks where it looks and right walks to the picture's right;
/// without one the axes are world-fixed.
pub fn camera_wish(yaw: Option<f32>, fwd: f32, right: f32) -> [f32; 2] {
    let Some(yaw) = yaw else {
        return [right, -fwd];
    };
    let (s, c) = yaw.to_radians().sin_cos();
    [fwd * s + right * c, fwd * c - right * s]
}

#[cfg(test)]
mod camera_wish_tests {
    use super::camera_wish;

    #[test]
    fn walking_follows_the_play_camera_only_when_the_view_looks_through_it() {
        assert_eq!(camera_wish(None, 1.0, 1.0), [1.0, -1.0]);
        // Yaw 0 looks along +z: forward walks +z, right walks +x (the picture's right).
        let w = camera_wish(Some(0.0), 1.0, 0.0);
        assert!(w[0].abs() < 1e-5 && (w[1] - 1.0).abs() < 1e-5);
        let r = camera_wish(Some(0.0), 0.0, 1.0);
        assert!((r[0] - 1.0).abs() < 1e-5 && r[1].abs() < 1e-5);
        // Turned 90 degrees, forward walks +x.
        let t = camera_wish(Some(90.0), 1.0, 0.0);
        assert!((t[0] - 1.0).abs() < 1e-5 && t[1].abs() < 1e-5);
    }
}

#[cfg(test)]
mod empty_scene_tests {
    use super::*;

    #[test]
    fn empty_scene_loads_back_with_one_named_root() {
        let text = World::empty_scene_text("New Scene");
        let world = World::from_text(&text).unwrap();
        assert_eq!(world.entities.len(), 1);
        assert_eq!(world.entities[0].name, "New Scene");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "play-preview")]
    fn play_drops_the_crate_and_stop_puts_it_back() {
        let mut world = World::demo();
        // The demo crate rests on the floor; lift it so it has somewhere to fall.
        world.entity_mut("crate").unwrap().translation[1] = 4.0;
        let start = world.entity("crate").unwrap().translation[1];
        world.play();
        assert!(world.step(0.2));
        assert!(world.entity("crate").unwrap().translation[1] < start);
        for _ in 0..180 {
            world.step(1.0 / 60.0);
        }
        let rested = world.entity("crate").unwrap().translation[1];
        assert!(rested > 0.2, "{rested}");
        assert!(rested < 1.2, "{rested}");
        world.stop();
        assert_eq!(world.entity("crate").unwrap().translation[1], start);
        assert_eq!(world.mode, PlayMode::Editing);
        assert!(!world.step(0.2));
    }

    #[test]
    #[cfg(not(feature = "play-preview"))]
    fn play_without_the_preview_feature_leaves_the_crate_where_it_is() {
        let mut world = World::demo();
        let start = world.entity("crate").unwrap().translation[1];
        world.play();
        assert!(!world.step(0.2));
        assert_eq!(world.entity("crate").unwrap().translation[1], start);
        world.stop();
        assert_eq!(world.entity("crate").unwrap().translation[1], start);
        assert!(!play_preview_enabled());
    }

    #[test]
    fn snap_floor_puts_the_crate_on_the_ground() {
        let mut world = World::demo();
        assert!(world.snap_floor("crate"));
        assert!((world.entity("crate").unwrap().translation[1] - 0.5).abs() < 0.01);
        world.entity_mut("crate").unwrap().scale[1] = 2.0;
        assert!(world.snap_floor("crate"));
        assert!((world.entity("crate").unwrap().translation[1] - 1.0).abs() < 0.01);
    }

    #[test]
    fn group_parents_the_selection_and_ungroup_puts_them_back() {
        let mut world = World::demo();
        let id = world.group(&["hero".into(), "crate".into()]);
        assert_eq!(world.entity("hero").unwrap().parent.as_deref(), Some(id.as_str()));
        assert!(world.ungroup(&id));
        assert_eq!(world.entity("hero").unwrap().parent.as_deref(), Some("root"));
        assert!(world.entity(&id).is_none());
    }

    #[test]
    fn a_scene_round_trips() {
        let world = World::demo();
        let loaded = World::from_text(&world.to_text()).unwrap();
        assert_eq!(loaded.entities.len(), world.entities.len());
        assert_eq!(loaded.entity("hero").unwrap().tag, "player");
        assert_eq!(loaded.entity("crate").unwrap().translation[1], 0.5);
        assert_eq!(loaded.gravity, DEFAULT_GRAVITY);
    }

    #[test]
    fn add_duplicate_and_remove_keep_ids_unique() {
        let mut world = World::demo();
        let cube = world.add("cube");
        assert_eq!(cube, "cube");
        assert_eq!(world.add("cube"), "cube-2");
        let copy = world.duplicate("cube-2").unwrap();
        assert_eq!(copy, "cube-3");
        assert_eq!(world.entity(&copy).unwrap().translation[0], 1.0);
        assert!(world.remove("cube"));
        assert!(!world.remove("root"));
        assert!(world.entity("cube").is_none());
        assert!(world.duplicate("root").is_none());
    }

    #[test]
    #[cfg(feature = "play-preview")]
    fn no_gravity_leaves_the_crate_in_the_air() {
        let mut world = World::demo();
        world.entity_mut("crate").unwrap().translation[1] = 4.0;
        world.gravity = 0.0;
        world.play();
        for _ in 0..30 {
            world.step(1.0 / 60.0);
        }
        assert!((world.entity("crate").unwrap().translation[1] - 4.0).abs() < 0.01);
    }

    #[test]
    #[cfg(feature = "play-preview")]
    fn rays_hit_the_nearest_object_and_impulses_move_bodies() {
        let mut world = World::demo();
        world.gravity = 0.0;
        world.play();
        let (id, t) = world.ray_cast([-10.0, 0.5, 0.0], [1.0, 0.0, 0.0], 100.0).expect("hit");
        assert!(t > 0.0 && t < 100.0, "{id} {t}");
        assert!(world.ray_cast([0.0, 500.0, 0.0], [0.0, 1.0, 0.0], 10.0).is_none());
        world.entity_mut("crate").unwrap().dynamic = true;
        world.stop();
        world.play();
        world.apply_impulse("crate", [5.0, 0.0, 0.0]);
        for _ in 0..10 {
            world.step(1.0 / 60.0);
        }
        assert!(world.entity("crate").unwrap().velocity[0] > 0.0);
    }

    #[test]
    #[cfg(feature = "play-preview")]
    fn a_bouncy_ball_rebounds_and_a_dull_one_does_not() {
        let rebound = |restitution: f32| {
            let mut world = World::demo();
            let ground = world.add("plane");
            world.entity_mut(&ground).unwrap().translation = [100.0, 0.0, 0.0];
            let id = world.add("sphere");
            {
                let e = world.entity_mut(&id).unwrap();
                e.dynamic = true;
                e.translation = [100.0, 3.0, 0.0];
                e.restitution = restitution;
            }
            world.play();
            let mut peak_after_hit = 0.0f32;
            let mut hit = false;
            for _ in 0..240 {
                world.step(1.0 / 60.0);
                let e = world.entity(&id).unwrap();
                if e.velocity[1] > 0.5 {
                    hit = true;
                }
                if hit {
                    peak_after_hit = peak_after_hit.max(e.translation[1]);
                }
            }
            peak_after_hit
        };
        assert!(rebound(0.9) > rebound(0.0) + 0.3);
    }
    #[test]
    #[cfg(feature = "play-preview")]
    fn a_cube_stack_settles_and_stays_up() {
        let mut world = World::demo();
        let ground = world.add("plane");
        world.entity_mut(&ground).unwrap().translation = [200.0, 0.0, 0.0];
        let mut ids = Vec::new();
        for i in 0..3 {
            let id = world.add("cube");
            let e = world.entity_mut(&id).unwrap();
            e.dynamic = true;
            e.translation = [200.0, 0.6 + 1.05 * i as f32, 0.0];
            ids.push(id);
        }
        world.play();
        for _ in 0..300 {
            world.step(1.0 / 60.0);
        }
        let ys: Vec<f32> = ids.iter().map(|i| world.entity(i).unwrap().translation[1]).collect();
        assert!(ys[0] < ys[1] && ys[1] < ys[2], "order kept: {ys:?}");
        assert!(ys[2] > 1.5, "top cube still on the stack: {ys:?}");
        for id in &ids {
            let x = world.entity(id).unwrap().translation[0];
            assert!((x - 200.0).abs() < 0.5, "stack stayed in place");
        }
    }
    #[test]
    #[cfg(feature = "play-preview")]
    fn a_character_walks_jumps_and_steps_up_a_low_ledge() {
        let rules = CharacterRules::default();
        let mut world = World::demo();
        let ground = world.add("plane");
        world.entity_mut(&ground).unwrap().translation = [300.0, 0.0, 0.0];
        let ledge = world.add("cube");
        {
            let e = world.entity_mut(&ledge).unwrap();
            e.translation = [303.0, 0.15, 0.0];
            e.scale = [2.0, 0.3, 2.0];
        }
        let hero = world.add("capsule");
        {
            let e = world.entity_mut(&hero).unwrap();
            e.dynamic = true;
            e.translation = [300.0, 1.0, 0.0];
        }
        world.play();
        let mut grounded = false;
        for _ in 0..60 {
            world.step(1.0 / 60.0);
            grounded = world.character_step(&hero, [0.0, 0.0], false, &rules);
        }
        assert!(grounded, "settles on the floor");
        let x0 = world.entity(&hero).unwrap().translation[0];
        for _ in 0..30 {
            world.step(1.0 / 60.0);
            world.character_step(&hero, [3.0, 0.0], false, &rules);
        }
        assert!(world.entity(&hero).unwrap().translation[0] > x0 + 1.0, "walks");
        let y_floor = world.entity(&hero).unwrap().translation[1];
        world.character_step(&hero, [0.0, 0.0], true, &rules);
        let mut peak = y_floor;
        for _ in 0..30 {
            world.step(1.0 / 60.0);
            peak = peak.max(world.entity(&hero).unwrap().translation[1]);
            world.character_step(&hero, [0.0, 0.0], false, &rules);
        }
        assert!(peak > y_floor + 0.5, "jump leaves the ground: {peak} vs {y_floor}");
        let mut top = 0.0f32;
        for _ in 0..120 {
            world.step(1.0 / 60.0);
            world.character_step(&hero, [3.0, 0.0], false, &rules);
            top = top.max(world.entity(&hero).unwrap().translation[1]);
        }
        assert!(
            top > y_floor + 0.2,
            "stepped onto the ledge (highest point {top}, floor {y_floor})"
        );
    }
}

/// Copy one Inspector field (by its Inspector name) from `from` to `to`.
pub(crate) fn copy_field(from: &Entity, to: &mut Entity, field: &str) {
    match field {
        "role" => to.tag = from.tag.clone(),
        "r" | "g" | "b" | "color" => to.color = from.color,
        "roughness" => to.roughness = from.roughness,
        "mesh_texture" | "mesh_layer" | "mesh_slice" | "mesh_tile_u" | "mesh_tile_v" => {
            mesh_surface::copy_one(from, to, field)
        }
        "metal" => to.metal = from.metal,
        "intensity" => to.intensity = from.intensity,
        "fov" => to.fov = from.fov,
        "near" => to.near = from.near,
        "far" => to.far = from.far,
        "shape" | "mesh" => to.shape = from.shape,
        "motion" => to.dynamic = from.dynamic,
        "friction" => to.friction = from.friction,
        "bounce" => to.restitution = from.restitution,
        "walk_speed" => to.walk_speed = from.walk_speed,
        "sprite" => to.sprite = from.sprite.clone(),
        "layer" => to.layer = from.layer,
        "frames" => to.frames = from.frames,
        "fps" => to.fps = from.fps,
        "animation" => to.animation = from.animation.clone(),
        "flip_x" => to.flip[0] = from.flip[0],
        "flip_y" => to.flip[1] = from.flip[1],
        "tint" => to.tint = [from.tint[0], from.tint[1], from.tint[2], to.tint[3]],
        "opacity" => to.tint[3] = from.tint[3],
        "order" => to.order = from.order,
        "slice" => to.slice = from.slice,
        "slice_mode" => to.slice_mode = from.slice_mode.clone(),
        "slice_size" => to.slice_size = from.slice_size,
        "pivot" => to.pivot = from.pivot,
        "sort_mode" => to.sort_mode = from.sort_mode,
        "normal_map" => to.normal_map = from.normal_map.clone(),
        "parallax" => to.parallax = from.parallax,
        "repeat_x" => to.repeat_x = from.repeat_x,
        "light2d" => to.light2d.kind = from.light2d.kind.clone(),
        "light_range" => to.light2d.range = from.light2d.range,
        "light_cone" => to.light2d.cone = from.light2d.cone,
        "light_shadows" => to.light2d.shadows = from.light2d.shadows,
        "camera2d" => to.camera2d.on = from.camera2d.on,
        "cam_follow" => to.camera2d.follow = from.camera2d.follow.clone(),
        "cam_smoothing" => to.camera2d.smoothing = from.camera2d.smoothing,
        "cam_zoom" => to.camera2d.zoom = from.camera2d.zoom,
        "cam_limits" => to.camera2d.limits = from.camera2d.limits,
        "cam_pixels" => to.camera2d.pixel_snap = from.camera2d.pixel_snap,
        "cam_cull_mask" => to.camera2d.cull_mask = from.camera2d.cull_mask,
        k if cam_drag::copy_field(from, to, k) => {}
        "audio_listener" => to.audio_listener.on = from.audio_listener.on,
        "listener_max_distance" => {
            to.audio_listener.max_distance = from.audio_listener.max_distance
        }
        "listener_rolloff" => to.audio_listener.rolloff = from.audio_listener.rolloff,
        k if shape2d::known(k) => {
            if let Some(text) = shape2d::field_text(from, k) {
                shape2d::apply(to, k, &text);
            }
        }
        k if sprite_shape::KEYS.contains(&k) => {
            to.sprite_shape.on = from.sprite_shape.on;
            if let Some(text) = sprite_shape::field_text(from, k) {
                sprite_shape::apply(to, k, &text);
            }
        }
        k if material2d::KEYS.contains(&k) => {
            if let Some(text) = material2d::field_text(from, k) {
                material2d::apply(to, k, &text);
            }
        }
        k if sprite_mask::KEYS.contains(&k) => {
            if let Some(text) = sprite_mask::field_text(from, k) {
                sprite_mask::apply(to, k, &text);
            }
        }
        k if nav2d::KEYS.contains(&k) => {
            if let Some(text) = nav2d::field_text(&from.nav2d, k) {
                nav2d::apply(&mut to.nav2d, k, &text);
            }
        }
        k if character2d::KEYS.contains(&k) => {
            if let Some(text) = character2d::field_text(&from.character2d, k) {
                character2d::apply(&mut to.character2d, k, &text);
            }
        }
        k if phys2d::KEYS.contains(&k) => {
            if let Some(text) = phys2d::field_text(&from.phys2d, k) {
                phys2d::apply(&mut to.phys2d, k, &text);
            }
        }
        k if area_effect::KEYS.contains(&k) => {
            if let Some(text) = area_effect::field_text(&from.area, &from.surface, k) {
                area_effect::apply(&mut to.area, &mut to.surface, k, &text);
            }
        }
        "script" => to.script = from.script.clone(),
        "script_order" => to.script_order = from.script_order,
        "script_group" => to.script_group = from.script_group.clone(),
        k if remote_transform::KEYS.contains(&k) => {
            if let Some(text) = remote_transform::field_text(&from.remote, k) {
                remote_transform::apply(&mut to.remote, k, &text);
            }
        }
        "collider_enabled" => to.collider_enabled = from.collider_enabled,
        k if volumes::KEYS.contains(&k) => {
            if let Some(text) = volumes::field_text(&from.volume, k) {
                volumes::apply(&mut to.volume, k, &text);
            }
        }
        "particles" => to.particles = from.particles.clone(),
        "socket" => to.socket = from.socket.clone(),
        _ => {}
    }
}

/// A sprite tint as `#RRGGBB`, or `#RRGGBBAA` when it is see-through.
pub fn tint_hex(tint: &[f32; 4]) -> String {
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    let mut out = format!("#{:02X}{:02X}{:02X}", byte(tint[0]), byte(tint[1]), byte(tint[2]));
    if byte(tint[3]) != 255 {
        out.push_str(&format!("{:02X}", byte(tint[3])));
    }
    out
}

/// Read `#RGB`, `#RRGGBB` or `#RRGGBBAA` (the `#` is optional) into red, green,
/// blue and opacity from 0 to 1.
pub fn parse_tint(text: &str) -> Option<[f32; 4]> {
    let hex = text.trim().trim_start_matches('#');
    let hex: String =
        if hex.len() == 3 { hex.chars().flat_map(|c| [c, c]).collect() } else { hex.to_string() };
    if !(hex.len() == 6 || hex.len() == 8) || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok().map(|b| b as f32 / 255.0);
    let alpha = if hex.len() == 8 { channel(6)? } else { 1.0 };
    Some([channel(0)?, channel(2)?, channel(4)?, alpha])
}

/// Nine-slice borders as `left, right, top, bottom` in pixels.
/// Read 2D camera limits: `left, right, bottom, top` (any numbers, left below
/// right and bottom below top), or nothing / all zero for no limits.
pub fn parse_limits(text: &str) -> Option<[f32; 4]> {
    let parts: Vec<f32> = text
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|p| !p.is_empty())
        .map(|p| p.parse::<f32>().ok().filter(|v| v.is_finite()))
        .collect::<Option<_>>()?;
    match parts.as_slice() {
        [] => Some([0.0; 4]),
        [l, r, b, t] if (*l, *r, *b, *t) == (0.0, 0.0, 0.0, 0.0) => Some([0.0; 4]),
        [l, r, b, t] if l < r && b < t => Some([*l, *r, *b, *t]),
        _ => None,
    }
}

pub fn parallax_text(p: &[f32; 2]) -> String {
    format!("{}, {}", format_num(p[0]), format_num(p[1]))
}

/// Read a parallax factor: one number for both directions, or `across, up`.
/// Each must be between 0 and 10.
pub fn parse_parallax(text: &str) -> Option<[f32; 2]> {
    let parts: Vec<f32> = text
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|p| !p.is_empty())
        .map(|p| p.parse::<f32>().ok().filter(|v| v.is_finite() && (0.0..=10.0).contains(v)))
        .collect::<Option<_>>()?;
    match parts.as_slice() {
        [all] => Some([*all; 2]),
        [x, y] => Some([*x, *y]),
        _ => None,
    }
}

pub fn slice_text(slice: &[f32; 4]) -> String {
    slice.iter().map(|v| format_num(*v)).collect::<Vec<_>>().join(", ")
}

/// Read nine-slice borders: one number for all four sides, or `left, right,
/// top, bottom` separated by commas or spaces. Negative numbers are refused.
pub fn parse_slice(text: &str) -> Option<[f32; 4]> {
    let parts: Vec<f32> = text
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|p| !p.is_empty())
        .map(|p| p.parse::<f32>().ok().filter(|v| v.is_finite() && *v >= 0.0))
        .collect::<Option<_>>()?;
    match parts.as_slice() {
        [] => Some([0.0; 4]),
        [all] => Some([*all; 4]),
        [l, r, t, b] => Some([*l, *r, *t, *b]),
        _ => None,
    }
}
