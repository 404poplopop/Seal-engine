//! Logic graphs running on scene objects during play.
//!
//! Each object whose `script` names a graph gets its own running copy. Play
//! fires the graph's `start` event once, then every step advances its clock,
//! which runs due delays and timers and fires `tick` with the step length.
//! Graphs see objects by number, never 0: objects in the scene when play
//! starts are numbered by their place plus one, and objects a graph spawns
//! get the next free number. A number is never reused within one play, so a
//! removed object stays gone. A graph spawns by asset name: the copy is made
//! from the first object of that asset as it stood when play started, is
//! marked as spawned in play, and runs its own script too. A removed object
//! hears `destroyed` before it leaves. After every step, objects with a shape
//! or picture whose boxes start or stop overlapping both hear
//! `overlap_begin` or `overlap_end` with the other object, plus `hit` when
//! either is a dynamic body. A graph that fails
//! stops, and the failure goes to the play log; the other graphs keep running.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use seal_graph::{Graph, GraphInstance, Host, Interp, Program, RunState, Ty, Value, V2, V3};

use crate::Entity;

fn path_to_rel(root: &Path, full: &Path) -> String {
    full.strip_prefix(root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| full.to_string_lossy().replace('\\', "/"))
}

/// One object riding on another during play: it keeps `offset` from its
/// parent's position after every step.
#[derive(Clone, Debug)]
pub struct Attach {
    pub child: String,
    pub parent: String,
    pub offset: [f32; 3],
    /// The outliner parent before attaching, put back on detach.
    pub was: Option<String>,
}

struct Run {
    object: u64,
    /// The project-relative graph file.
    path: String,
    graph: Interp,
    failed: bool,
}

/// Where play stopped at a breakpoint.
#[derive(Clone, Debug, PartialEq)]
pub struct BreakHit {
    pub path: String,
    pub node: u32,
    pub object: String,
}

/// What the debugger shows for one graph file during play.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DebugView {
    /// The node the run is stopped before, when paused in this graph.
    pub paused_at: Option<u32>,
    /// Pending nodes of the paused run, innermost first.
    pub call_stack: Vec<u32>,
    /// Nodes and the play step they last ran in.
    pub ran: BTreeMap<u32, u64>,
    /// The play step now.
    pub step: u64,
    /// Variables of the object being watched: name and value.
    pub vars: Vec<(String, String)>,
    /// Last values of data outputs: (node, output index) and value.
    pub values: BTreeMap<(u32, u32), String>,
    /// The object whose values are shown.
    pub object: String,
    /// Current state of the graph's state machine, empty when it has none.
    pub state: String,
}

/// Object numbers for graphs, kept stable while objects come and go.
#[derive(Default)]
pub struct Handles {
    ids: HashMap<u64, String>,
    next: u64,
    /// Removed by a graph; still readable until its `destroyed` event ran.
    dying: Vec<u64>,
}

impl Handles {
    fn index(&self, entities: &[Entity], handle: u64) -> Option<usize> {
        let id = self.ids.get(&handle)?;
        entities.iter().position(|e| &e.id == id)
    }

    fn handle(&self, entities: &[Entity], index: usize) -> u64 {
        let id = &entities[index].id;
        self.ids.iter().find(|(_, v)| *v == id).map_or(seal_graph::rt::NO_ENTITY, |(k, _)| *k)
    }

    fn add(&mut self, id: String) -> u64 {
        self.next += 1;
        self.ids.insert(self.next, id);
        self.next
    }
}

#[derive(Default)]
pub struct Phys2dPending {
    pub force: Vec<(String, [f32; 2])>,
    pub torque: Vec<(String, f32)>,
    pub velocity: Vec<(String, [f32; 2], f32)>,
    pub joint_target: Vec<(String, [f32; 2])>,
}

pub struct Scripts {
    runs: Vec<Run>,
    handles: Handles,
    templates: HashMap<String, Entity>,
    programs: HashMap<String, Result<Program, String>>,
    project: Option<PathBuf>,
    /// Pairs of objects (lower number first) overlapping after the last step.
    contacts: Vec<(u64, u64)>,
    /// Parsed `.soundset` files, keyed by project-relative path.
    container_defs: HashMap<String, crate::sound_container::SoundContainerDef>,
    /// Playback state per sound container path.
    container_runtimes: HashMap<String, crate::sound_container::SoundContainerRuntime>,
    /// Scene id of an area to the scene ids of dynamic bodies inside it.
    area_inside: BTreeMap<String, Vec<String>>,
    /// Keys held down now, by graph key name.
    held: std::collections::BTreeSet<String>,
    attached: Vec<Attach>,
    /// Sounds started since the world last collected them.
    pub sounds: Vec<SoundCue>,
    /// Music commands since the world last collected them.
    pub music: Vec<MusicCue>,
    /// Animation requests from graphs: object id and an .anim path (None = stop).
    /// An empty object id is a whole-scene sequence request with a .seq path.
    pub anim_cmds: Vec<(String, Option<String>)>,
    /// AI orders from graphs, by object id, for the world to carry out.
    pub ai_cmds: Vec<(String, crate::ai::Goal)>,
    /// Character 2D move and jump orders from graphs, applied on the next motion step.
    pub char_cmds: Vec<crate::character2d::Cmd>,
    /// Character 2D floor, wall and velocity from the previous step, by object id.
    pub char_view: BTreeMap<String, crate::character2d::View>,
    /// One-frame 2D physics commands from graphs; drained before each physics step.
    pub phys2d: Phys2dPending,
    /// Handle-based physics shared with the scene. Graph nodes create bodies here.
    physics: Arc<Mutex<crate::physics::PhysicsServer>>,
    /// Objects whose AI order finished, as the world last reported.
    pub ai_arrived: BTreeSet<String>,
    /// Who controls what, scores and the game's state.
    pub game: crate::game::Game,
    /// The on-screen widgets scripts built.
    pub ui: GameUi,
    /// Breakpoint node ids by graph file.
    pub breakpoints: BTreeMap<String, BTreeSet<u32>>,
    /// Set when a run stopped at a breakpoint; play stays paused until it continues.
    pub hit: Option<(u64, BreakHit)>,
    /// Nodes run, by graph file, and the step they last ran in.
    ran: BTreeMap<String, BTreeMap<u32, u64>>,
    steps: u64,
    /// Clock for unscaled_time, unscaled_dt and sim_tick graph nodes.
    clock_unscaled_time: f64,
    clock_unscaled_dt: f64,
    clock_sim_tick: i64,
}

/// On-screen widgets during play: which are shown and the text elements
/// scripts set. Widget layouts are not imported, so shown widgets display
/// their text elements as lines.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GameUi {
    pub shown: BTreeSet<String>,
    pub texts: BTreeMap<String, String>,
    /// Camera shakes scripts asked for since the last step: strength, seconds.
    pub shakes: Vec<(f32, f32)>,
    /// Each on-screen control's value and text by name, for `ui_value` and `ui_input`.
    pub values: BTreeMap<String, (f64, String)>,
    /// Input actions held this step, from the project's bindings.
    pub actions: BTreeMap<String, bool>,
    /// Input axes from the project's bindings, -1..1.
    pub axes: BTreeMap<String, f32>,
    /// Pad rumbles scripts asked for since the last step: strong, weak, seconds.
    pub rumbles: Vec<(f32, f32, f32)>,
    /// Rebinding waits scripts asked for: action, slot.
    pub rebinds: Vec<(String, usize)>,
    /// Save, load and delete requests from scripts, for the world to carry out.
    pub saves: Vec<seal_graph::rt::GameCmd>,
    /// Save slots that hold a game, as the world last listed them.
    pub slots: BTreeSet<String>,
    /// Scene opens, adds, background loads and unloads, for the world to carry out.
    pub scene_cmds: Vec<seal_graph::rt::GameCmd>,
    /// Keep-across-scenes changes: object id, keep.
    pub keeps: Vec<(String, bool)>,
    /// The scenes in the play, the open one first, as the world last listed them.
    pub scenes: Vec<String>,
    /// `play.time_scale` from a script, applied by the world on the next step.
    pub time_scale: Option<f32>,
    /// Typewriter restarts and skips waiting for the next screen step: element, skip.
    pub typewriter: Vec<(String, bool)>,
    /// String-table keys already reported missing, as `table` then a zero byte then `key`.
    pub string_warned: BTreeSet<String>,
    /// Seconds each action has been held after its modifiers.
    pub held_for: BTreeMap<String, f32>,
    /// Actions that went up this step.
    pub released: BTreeMap<String, bool>,
    /// Pointer movement this step, in screen pixels.
    pub mouse_delta: [f32; 2],
    /// Unaccelerated pointer motion this step.
    pub raw_mouse: [f32; 2],
    /// Motion used for mouse look this step.
    pub look_delta: [f32; 2],
    /// Pen pressure, 0 to 1.
    pub pen_pressure: f32,
    /// Pen tilt, each axis -1 to 1.
    pub pen_tilt: [f32; 2],
    /// Whether the eraser end is down.
    pub pen_eraser: bool,
    /// Gamepad rotation speed, radians per second.
    pub pad_gyro: [f32; 3],
    /// Gamepad acceleration, in gravities.
    pub pad_accel: [f32; 3],
    /// Why motion sensors are missing. Empty when they were read.
    pub pad_motion_note: String,
    /// Why the light bar cannot be set. Empty when it can.
    pub pad_light_note: String,
    /// Light-bar colour a script asked for this step, channels 0 to 1.
    pub pad_light: Option<[f32; 3]>,
    /// Mouse wheel this step.
    pub mouse_wheel: f32,
    /// Pointer position in the flat scene, in scene units.
    pub pointer_world: [f32; 2],
    /// The built-in dialog a script is showing.
    pub dialog: Option<crate::dialog_ui::Ask>,
    /// Button from the last dialog answer: yes, no, or ok.
    pub dialog_button: String,
    /// Text from the last dialog answer.
    pub dialog_text: String,
    /// Cloud transfers scripts asked for this step.
    pub cloud: Vec<crate::cloud::Cmd>,
    /// The cloud job a script hook is handling.
    pub cloud_job: Option<crate::cloud::Job>,
    /// Which side won the last cloud conflict: local or remote.
    pub cloud_kept: String,
}

impl GameUi {
    /// The lines to draw over the viewport, by element name.
    pub fn lines(&self) -> Vec<String> {
        if self.shown.is_empty() {
            return Vec::new();
        }
        self.texts.values().filter(|t| !t.is_empty()).cloned().collect()
    }
}

/// A sound a script started: project path, volume and pitch.
#[derive(Clone, Debug, PartialEq)]
pub struct SoundCue {
    pub sound: String,
    pub volume: f32,
    pub pitch: f32,
    /// World point for positioned sounds.
    pub at: Option<[f32; 3]>,
}

/// A music-bus command from a script, or from the scene's music file.
#[derive(Clone, Debug, PartialEq)]
pub enum MusicCue {
    /// `loop_start` is seconds from the start of the file. `fade` is seconds.
    Play {
        file: String,
        looping: bool,
        fade: f32,
        loop_start: f32,
    },
    Stop {
        fade: f32,
    },
    Volume {
        volume: f32,
    },
    Pause,
    Resume,
}

/// The input axes every project has, by name: each lists the keys that push
/// it up (+1) and down (-1). Opposite keys held together cancel out.
pub const AXES: [(&str, &[&str], &[&str]); 3] = [
    ("MoveForward", &["W", "Up"], &["S", "Down"]),
    ("MoveRight", &["D", "Right"], &["A", "Left"]),
    ("MoveUp", &["E", "SpaceBar"], &["Q"]),
];

/// The value of input axis `name` with `held` keys down: -1, 0 or 1.
pub fn axis_value(name: &str, held: &std::collections::BTreeSet<String>) -> f64 {
    let Some((_, up, down)) = AXES.iter().find(|(n, _, _)| *n == name) else { return 0.0 };
    let any = |keys: &[&str]| keys.iter().any(|k| held.contains(*k));
    f64::from(i8::from(any(up)) - i8::from(any(down)))
}

fn arr(v: V3) -> [f32; 3] {
    [v.x as f32, v.y as f32, v.z as f32]
}

fn v3(a: [f32; 3]) -> Value {
    Value::Vec3(V3 { x: a[0] as f64, y: a[1] as f64, z: a[2] as f64 })
}

fn v2(a: [f32; 2]) -> seal_graph::rt::V2 {
    seal_graph::rt::V2 { x: f64::from(a[0]), y: f64::from(a[1]) }
}

fn v3_rt(a: [f32; 3]) -> seal_graph::rt::V3 {
    seal_graph::rt::V3 { x: f64::from(a[0]), y: f64::from(a[1]), z: f64::from(a[2]) }
}

/// Half the size of an object's collision box, or `None` when it has no
/// shape or picture to touch with. Shapes are unit sized before scale;
/// a plane is a wide thin slab.
type TouchBox = (u64, [f32; 3], [f32; 3], bool);

/// The middle of the region two touching boxes share.
fn contact_point(a: &TouchBox, b: &TouchBox) -> [f64; 3] {
    std::array::from_fn(|k| {
        let lo = (a.1[k] - a.2[k]).max(b.1[k] - b.2[k]) as f64;
        let hi = (a.1[k] + a.2[k]).min(b.1[k] + b.2[k]) as f64;
        (lo + hi) / 2.0
    })
}

/// The unit axis along which the boxes overlap least, pointing from `b`
/// towards `a`: the way `a` would be pushed out.
fn contact_normal(a: &TouchBox, b: &TouchBox) -> [f64; 3] {
    let depth = |k: usize| a.2[k] + b.2[k] - (a.1[k] - b.1[k]).abs();
    let k = (0..3).fold(0, |best, k| if depth(k) < depth(best) { k } else { best });
    let mut n = [0.0; 3];
    n[k] = if a.1[k] >= b.1[k] { 1.0 } else { -1.0 };
    n
}

fn collision_half(e: &Entity) -> Option<[f32; 3]> {
    let s = e.scale.map(|v| v.abs().max(0.05));
    match e.shape {
        Some(crate::Shape::Plane) => Some([20.0 * s[0], 0.05 * s[1], 20.0 * s[2]]),
        Some(_) => Some(s.map(|v| 0.5 * v)),
        None if !e.sprite.trim().is_empty() => Some(s.map(|v| 0.5 * v)),
        None => None,
    }
}

/// The scene as the world a graph reads and changes.
pub struct SceneHost<'a> {
    pub entities: &'a mut Vec<Entity>,
    pub log: &'a mut Vec<String>,
    handles: &'a mut Handles,
    templates: &'a HashMap<String, Entity>,
    /// Objects spawned during this call, whose scripts start after it.
    spawned: Vec<u64>,
    held: &'a std::collections::BTreeSet<String>,
    attached: &'a mut Vec<Attach>,
    sounds: &'a mut Vec<SoundCue>,
    music: &'a mut Vec<MusicCue>,
    anim_cmds: &'a mut Vec<(String, Option<String>)>,
    ai_cmds: &'a mut Vec<(String, crate::ai::Goal)>,
    ai_arrived: &'a BTreeSet<String>,
    char_cmds: &'a mut Vec<crate::character2d::Cmd>,
    char_view: &'a BTreeMap<String, crate::character2d::View>,
    game: &'a mut crate::game::Game,
    ui: &'a mut GameUi,
    project: Option<&'a Path>,
    container_defs: &'a mut HashMap<String, crate::sound_container::SoundContainerDef>,
    container_runtimes: &'a mut HashMap<String, crate::sound_container::SoundContainerRuntime>,
    area_inside: &'a BTreeMap<String, Vec<String>>,
    phys2d: &'a mut Phys2dPending,
    unscaled_time: f64,
    unscaled_dt: f64,
    sim_tick: i64,
    physics: Arc<Mutex<crate::physics::PhysicsServer>>,
}

fn finite32(v: f64) -> f32 {
    let n = v as f32;
    if n.is_finite() {
        n
    } else {
        0.0
    }
}

impl SceneHost<'_> {
    fn scene_id(&self, entity: u64) -> Result<String, String> {
        let i = self
            .handles
            .index(self.entities, entity)
            .ok_or_else(|| format!("no object {entity}"))?;
        Ok(self.entities[i].id.clone())
    }

    fn char_flag(&self, entity: u64, wall: bool) -> bool {
        let Ok(id) = self.scene_id(entity) else { return false };
        self.char_view.get(&id).is_some_and(|v| if wall { v.on_wall } else { v.on_floor })
    }

    fn object(&mut self, entity: u64) -> Result<&mut Entity, String> {
        let i = self
            .handles
            .index(self.entities, entity)
            .ok_or_else(|| format!("no object {entity}"))?;
        Ok(&mut self.entities[i])
    }

    /// `attach.snap`, `attach.keep_world` or `attach.keep_relative` set to a
    /// parent attaches the object to it; `attach.detach` lets it go.
    fn attach(&mut self, entity: u64, rule: &str, value: Value) -> Result<(), String> {
        let child = self.object(entity)?.clone();
        if let Some(i) = self.attached.iter().position(|a| a.child == child.id) {
            let old = self.attached.remove(i);
            self.object(entity)?.parent = old.was;
        }
        if rule == "detach" {
            return Ok(());
        }
        let Value::Entity(p) = value else { return Err(format!("attach.{rule} needs an object")) };
        if p == seal_graph::rt::NO_ENTITY {
            return Ok(());
        }
        let parent = self.object(p)?.clone();
        // No loops: the parent may not ride on the child, however indirectly.
        let mut up = Some(parent.id.clone());
        while let Some(id) = up {
            if id == child.id {
                return Err(format!("{} cannot be attached to itself", child.name));
            }
            up = self.attached.iter().find(|a| a.child == id).map(|a| a.parent.clone());
        }
        let sub = |a: [f32; 3], b: [f32; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
        let offset = match rule {
            "snap" => [0.0; 3],
            "keep_world" => sub(child.translation, parent.translation),
            "keep_relative" => child.translation,
            _ => return Err(format!("objects have no attach.{rule}")),
        };
        self.attached.push(Attach {
            child: child.id.clone(),
            parent: parent.id.clone(),
            offset,
            was: child.parent.clone(),
        });
        let e = self.object(entity)?;
        e.parent = Some(parent.id);
        e.translation = [
            parent.translation[0] + offset[0],
            parent.translation[1] + offset[1],
            parent.translation[2] + offset[2],
        ];
        Ok(())
    }

    fn free_id(&self, base: &str) -> String {
        (2..)
            .map(|n| format!("{base}-{n}"))
            .find(|id| !self.entities.iter().any(|e| &e.id == id))
            .unwrap_or_default()
    }

    fn play_container(
        &mut self,
        rel: &str,
        volume: f32,
        pitch: f32,
    ) -> Result<Vec<crate::sound_container::PlayPick>, String> {
        let key = rel.trim().replace('\\', "/");
        if !self.container_defs.contains_key(&key) {
            let root = self.project.ok_or_else(|| "play sound: no project folder".to_string())?;
            let path = root.join(key.trim_start_matches(['/', '\\']));
            let text = std::fs::read_to_string(&path).map_err(|e| format!("{key}: {e}"))?;
            let def = crate::sound_container::SoundContainerDef::parse(&text)
                .map_err(|e| format!("{key}: {e}"))?;
            self.container_defs.insert(key.clone(), def);
        }
        let def = self.container_defs.get(&key).cloned().unwrap();
        let runtime = self
            .container_runtimes
            .entry(key.clone())
            .or_insert_with(|| crate::sound_container::SoundContainerRuntime::new(def));
        let picks = runtime.play(volume, pitch).map_err(|e| e.to_string())?;
        let root = self.project.ok_or_else(|| "no project".to_string())?;
        let set_dir = root.join(key.trim_start_matches(['/', '\\']));
        let set_dir = set_dir.parent().unwrap_or(root);
        Ok(picks
            .into_iter()
            .map(|mut p| {
                let candidate = set_dir.join(&p.path);
                if candidate.is_file() {
                    p.path = path_to_rel(root, &candidate);
                } else {
                    let alt = root.join(&p.path);
                    if alt.is_file() {
                        p.path = path_to_rel(root, &alt);
                    }
                }
                p
            })
            .collect())
    }
}

impl Host for SceneHost<'_> {
    fn get(&mut self, entity: u64, component: &str, field: &str, ty: Ty) -> Result<Value, String> {
        if (component, field) == ("attach", "parent") {
            let id = self.object(entity)?.id.clone();
            let parent = self.attached.iter().find(|a| a.child == id).map(|a| a.parent.clone());
            let handle = parent
                .and_then(|p| self.entities.iter().position(|e| e.id == p))
                .map_or(seal_graph::rt::NO_ENTITY, |i| self.handles.handle(self.entities, i));
            return Ok(Value::Entity(handle));
        }
        let e = self.object(entity)?;
        let f = |x: f32| Value::Float(x as f64);
        let v = match (component, field) {
            ("transform", "translation") => v3(e.translation),
            ("transform", "rotation") => v3(e.rotation),
            ("transform", "scale") => v3(e.scale),
            ("look", "color") => v3(e.color),
            ("look", "roughness") => f(e.roughness),
            ("look", "metal") => f(e.metal),
            ("sprite", "hidden") => Value::Bool(e.hidden),
            ("sprite_image", "image") => Value::Str(e.sprite.clone()),
            ("sprite_image", "layer") => Value::Int(e.layer as i64),
            ("sprite_image", "frames") => Value::Int(e.frames as i64),
            ("sprite_image", "fps") => f(e.fps),
            ("body", "dynamic") => Value::Bool(e.dynamic),
            ("body", "velocity") => v3(e.velocity),
            ("light", "intensity") => f(e.intensity),
            ("camera", "fov") => f(e.fov),
            ("camera", "near") => f(e.near),
            ("camera", "far") => f(e.far),
            ("editor", "name") => Value::Str(e.name.clone()),
            ("editor", "tag") => Value::Str(e.tag.clone()),
            _ => return Err(format!("objects have no {component}.{field}")),
        };
        if v.ty() != ty {
            return Err(format!("{component}.{field} is a {}, not a {ty}", v.ty()));
        }
        Ok(v)
    }

    fn set(
        &mut self,
        entity: u64,
        component: &str,
        field: &str,
        value: Value,
    ) -> Result<(), String> {
        if (component, field) == ("play", "time_scale") {
            let Value::Float(x) = value else {
                return Err("play.time_scale is a float".into());
            };
            self.ui.time_scale = Some((x as f32).max(0.0));
            return Ok(());
        }
        if component == "attach" {
            return self.attach(entity, field, value);
        }
        let e = self.object(entity)?;
        match (component, field, value) {
            ("sprite", "hidden", Value::Bool(b)) => e.hidden = b,
            ("body", "dynamic", Value::Bool(b)) => e.dynamic = b,
            ("transform", "translation", Value::Vec3(v)) => e.translation = arr(v),
            ("transform", "rotation", Value::Vec3(v)) => e.rotation = arr(v),
            ("transform", "scale", Value::Vec3(v)) => e.scale = arr(v),
            ("look", "color", Value::Vec3(v)) => e.color = arr(v),
            ("body", "velocity", Value::Vec3(v)) => e.velocity = arr(v),
            ("look", "roughness", Value::Float(x)) => e.roughness = x as f32,
            ("look", "metal", Value::Float(x)) => e.metal = x as f32,
            ("light", "intensity", Value::Float(x)) => e.intensity = x as f32,
            ("camera", "fov", Value::Float(x)) => e.fov = x as f32,
            ("camera", "near", Value::Float(x)) => e.near = x as f32,
            ("camera", "far", Value::Float(x)) => e.far = x as f32,
            ("sprite_image", "image", Value::Str(t)) => e.sprite = t,
            ("sprite_image", "layer", Value::Int(i)) => e.layer = i as i32,
            ("sprite_image", "frames", Value::Int(i)) => e.frames = i.max(1) as u32,
            ("sprite_image", "fps", Value::Float(x)) => e.fps = x as f32,
            ("editor", "name", Value::Str(t)) => e.name = t,
            ("editor", "tag", Value::Str(t)) => e.tag = t,
            (_, _, value) => return Err(format!("cannot set {component}.{field} to {value}")),
        }
        Ok(())
    }

    fn log(&mut self, message: &str) {
        self.log.push(message.to_string());
    }

    fn spawn(&mut self, prefab: &str) -> Result<u64, String> {
        let template = self
            .templates
            .get(prefab)
            .ok_or_else(|| format!("nothing in the scene is a {prefab} to spawn"))?;
        let mut copy = template.clone();
        copy.id = self.free_id(&template.id);
        copy.outliner_play_spawned = true;
        let handle = self.handles.add(copy.id.clone());
        self.entities.push(copy);
        self.spawned.push(handle);
        Ok(handle)
    }

    fn despawn(&mut self, entity: u64) -> Result<(), String> {
        if self.handles.index(self.entities, entity).is_some()
            && !self.handles.dying.contains(&entity)
        {
            self.handles.dying.push(entity);
        }
        Ok(())
    }

    fn exists(&mut self, entity: u64) -> bool {
        self.handles.index(self.entities, entity).is_some() && !self.handles.dying.contains(&entity)
    }

    fn is_a(&mut self, entity: u64, prefab: &str) -> bool {
        self.object(entity).map(|e| e.source_asset == prefab).unwrap_or(false)
    }

    fn find_all(&mut self, prefab: &str) -> Vec<u64> {
        (0..self.entities.len())
            .filter(|&i| self.entities[i].source_asset == prefab)
            .map(|i| self.handles.handle(self.entities, i))
            .filter(|&h| h != seal_graph::rt::NO_ENTITY && !self.handles.dying.contains(&h))
            .collect()
    }

    fn axis(&mut self, name: &str) -> f64 {
        // A bound axis, or an action pair named like it, wins over the built-in key axes.
        if let Some(v) = self.ui.axes.get(name) {
            if *v != 0.0 {
                return f64::from(*v);
            }
        }
        axis_value(name, self.held)
    }

    fn action_pressed(&mut self, name: &str) -> bool {
        self.ui.actions.get(name.trim()).copied().unwrap_or(false)
    }

    fn action_held(&mut self, name: &str) -> f64 {
        f64::from(self.ui.held_for.get(name.trim()).copied().unwrap_or(0.0))
    }

    fn action_released(&mut self, name: &str) -> bool {
        self.ui.released.get(name.trim()).copied().unwrap_or(false)
    }

    fn mouse_delta(&mut self) -> seal_graph::rt::V2 {
        seal_graph::rt::V2 {
            x: f64::from(self.ui.mouse_delta[0]),
            y: f64::from(self.ui.mouse_delta[1]),
        }
    }

    fn raw_mouse_delta(&mut self) -> seal_graph::rt::V2 {
        v2(self.ui.raw_mouse)
    }

    fn mouse_look(&mut self) -> seal_graph::rt::V2 {
        v2(self.ui.look_delta)
    }

    fn pen_pressure(&mut self) -> f64 {
        f64::from(self.ui.pen_pressure)
    }

    fn pen_tilt(&mut self) -> seal_graph::rt::V2 {
        v2(self.ui.pen_tilt)
    }

    fn pen_eraser(&mut self) -> bool {
        self.ui.pen_eraser
    }

    fn pad_gyro(&mut self) -> seal_graph::rt::V3 {
        v3_rt(self.ui.pad_gyro)
    }

    fn pad_accel(&mut self) -> seal_graph::rt::V3 {
        v3_rt(self.ui.pad_accel)
    }

    fn pad_motion_note(&mut self) -> String {
        self.ui.pad_motion_note.clone()
    }

    fn set_pad_light(&mut self, red: f64, green: f64, blue: f64) -> Result<(), String> {
        let channel = |v: f64| {
            let n = v as f32;
            if n.is_finite() {
                n.clamp(0.0, 1.0)
            } else {
                0.0
            }
        };
        self.ui.pad_light = Some([channel(red), channel(green), channel(blue)]);
        if self.ui.pad_light_note.is_empty() {
            Ok(())
        } else {
            Err(self.ui.pad_light_note.clone())
        }
    }

    fn mouse_wheel(&mut self) -> f64 {
        f64::from(self.ui.mouse_wheel)
    }

    fn pointer_world(&mut self) -> seal_graph::rt::V2 {
        seal_graph::rt::V2 {
            x: f64::from(self.ui.pointer_world[0]),
            y: f64::from(self.ui.pointer_world[1]),
        }
    }

    fn rumble(&mut self, strong: f64, weak: f64, seconds: f64) -> Result<(), String> {
        self.ui.rumbles.push((strong as f32, weak as f32, seconds as f32));
        Ok(())
    }

    fn rebind_action(&mut self, action: &str, slot: usize) -> Result<(), String> {
        self.ui.rebinds.push((action.to_string(), slot));
        Ok(())
    }

    fn ui_show(&mut self, widget: &str, visible: bool) -> Result<(), String> {
        if visible {
            self.ui.shown.insert(widget.to_string());
        } else {
            self.ui.shown.remove(widget);
        }
        Ok(())
    }

    fn ui_text(&mut self, element: &str, text: &str) -> Result<(), String> {
        self.ui.texts.insert(element.to_string(), text.to_string());
        Ok(())
    }

    fn get_string(&mut self, table: &str, key: &str, values: &str) -> String {
        let Some(root) = self.project else { return key.to_string() };
        let Ok(loaded) = crate::text_sprites::load_table_cached(&root.join(table)) else {
            return key.to_string();
        };
        let pairs = crate::text_sprites::parse_assignments(values);
        let hit = crate::text_sprites::lookup(&loaded, key, &pairs);
        if hit.missing {
            let mark = format!("{table}\0{key}");
            if self.ui.string_warned.insert(mark) {
                self.log.push(crate::text_sprites::missing_key_warning(key));
            }
        }
        hit.text
    }

    fn typewriter(&mut self, element: &str, skip: bool) -> Result<(), String> {
        self.ui.typewriter.push((element.to_string(), skip));
        Ok(())
    }

    fn camera_shake(&mut self, strength: f64, seconds: f64) -> Result<(), String> {
        self.ui.shakes.push((strength as f32, seconds as f32));
        Ok(())
    }

    fn query_shapes(&mut self) -> Vec<seal_graph::query::Shape> {
        let entities = &self.entities;
        let handles = &self.handles;
        crate::query_shapes::from_entities(entities, |i| handles.handle(entities, i))
    }

    fn bodies_inside(&mut self, entity: u64) -> Vec<u64> {
        let Some(i) = self.handles.index(self.entities, entity) else { return Vec::new() };
        let id = self.entities[i].id.clone();
        let Some(names) = self.area_inside.get(&id) else { return Vec::new() };
        names
            .iter()
            .filter_map(|name| self.entities.iter().position(|e| &e.id == name))
            .map(|i| self.handles.handle(self.entities, i))
            .filter(|h| *h != seal_graph::rt::NO_ENTITY)
            .collect()
    }

    fn ray_cast(&mut self, from: V3, dir: V3, max: f64) -> Option<(u64, f64)> {
        let len = (dir.x * dir.x + dir.y * dir.y + dir.z * dir.z).sqrt();
        if len < 1e-9 {
            return None;
        }
        let d = [dir.x / len, dir.y / len, dir.z / len];
        let o = [from.x, from.y, from.z];
        let mut best: Option<(usize, f64)> = None;
        for (i, e) in self.entities.iter().enumerate() {
            let c = [e.translation[0] as f64, e.translation[1] as f64, e.translation[2] as f64];
            let half = [e.scale[0] as f64 * 0.5, e.scale[1] as f64 * 0.5, e.scale[2] as f64 * 0.5];
            let t = if e.source_kind.contains("sphere") {
                let r = half[0].max(half[1]).max(half[2]);
                let oc = [o[0] - c[0], o[1] - c[1], o[2] - c[2]];
                let b = oc[0] * d[0] + oc[1] * d[1] + oc[2] * d[2];
                let cc = oc[0] * oc[0] + oc[1] * oc[1] + oc[2] * oc[2] - r * r;
                let disc = b * b - cc;
                if disc < 0.0 {
                    None
                } else {
                    let s = disc.sqrt();
                    if -b - s >= 0.0 {
                        Some(-b - s)
                    } else if -b + s >= 0.0 {
                        Some(0.0)
                    } else {
                        None
                    }
                }
            } else {
                let (mut t0, mut t1) = (0.0f64, f64::INFINITY);
                let mut ok = true;
                for k in 0..3 {
                    let (lo, hi) = (c[k] - half[k], c[k] + half[k]);
                    if d[k].abs() < 1e-12 {
                        if o[k] < lo || o[k] > hi {
                            ok = false;
                        }
                    } else {
                        let (a, b) = ((lo - o[k]) / d[k], (hi - o[k]) / d[k]);
                        t0 = t0.max(a.min(b));
                        t1 = t1.min(a.max(b));
                    }
                }
                if ok && t0 <= t1 {
                    Some(t0)
                } else {
                    None
                }
            };
            if let Some(t) = t {
                if t <= max && best.is_none_or(|b| t < b.1) {
                    best = Some((i, t));
                }
            }
        }
        let (i, t) = best?;
        let h = self.handles.handle(self.entities, i);
        if h == seal_graph::rt::NO_ENTITY || self.handles.dying.contains(&h) {
            return None;
        }
        Some((h, t))
    }

    fn physics(&mut self, op: &str, args: &[Value]) -> Result<Value, String> {
        let mut server = crate::physics::lock(&self.physics);
        crate::physics::apply_call(&mut server, self.project, op, args)
    }

    fn character_move(&mut self, entity: u64, wish: V2) -> Result<(), String> {
        let id = self.scene_id(entity)?;
        if !self.entities.iter().any(|e| e.id == id && e.character2d.on) {
            return Err(format!("{id} has no Character 2D"));
        }
        let wish = [finite32(wish.x), finite32(wish.y)];
        self.char_cmds.push(crate::character2d::Cmd::Move { id, wish });
        Ok(())
    }

    fn character_jump(&mut self, entity: u64) -> Result<(), String> {
        let id = self.scene_id(entity)?;
        if !self.entities.iter().any(|e| e.id == id && e.character2d.on) {
            return Err(format!("{id} has no Character 2D"));
        }
        self.char_cmds.push(crate::character2d::Cmd::Jump { id });
        Ok(())
    }

    fn character_mode(&mut self, entity: u64, mode: &str) -> Result<(), String> {
        if !crate::character2d::MODES.contains(&mode) {
            return Err(format!("movement mode {mode:?} must be walk, fly, swim or climb"));
        }
        let id = self.scene_id(entity)?;
        let e = self
            .entities
            .iter_mut()
            .find(|e| e.id == id)
            .ok_or_else(|| format!("no object {entity}"))?;
        if !e.character2d.on {
            return Err(format!("{id} has no Character 2D"));
        }
        e.character2d.mode = mode.to_string();
        Ok(())
    }

    fn character_on_floor(&mut self, entity: u64) -> bool {
        self.char_flag(entity, false)
    }

    fn character_on_wall(&mut self, entity: u64) -> bool {
        self.char_flag(entity, true)
    }

    fn character_velocity(&mut self, entity: u64) -> V2 {
        let Ok(id) = self.scene_id(entity) else {
            return V2 { x: 0.0, y: 0.0 };
        };
        self.char_view.get(&id).map_or(V2 { x: 0.0, y: 0.0 }, |v| V2 {
            x: v.velocity[0] as f64,
            y: v.velocity[1] as f64,
        })
    }

    fn apply_impulse(&mut self, entity: u64, impulse: V3) -> Result<(), String> {
        if let Ok(e) = self.object(entity) {
            if e.dynamic {
                e.velocity[0] += impulse.x as f32;
                e.velocity[1] += impulse.y as f32;
                e.velocity[2] += impulse.z as f32;
            }
        }
        Ok(())
    }

    fn apply_force_2d(&mut self, entity: u64, force: seal_graph::rt::V2) -> Result<(), String> {
        let id = self.object(entity)?.id.clone();
        self.phys2d.force.push((id, [force.x as f32, force.y as f32]));
        Ok(())
    }

    fn apply_torque_2d(&mut self, entity: u64, torque: f64) -> Result<(), String> {
        let id = self.object(entity)?.id.clone();
        self.phys2d.torque.push((id, torque as f32));
        Ok(())
    }

    fn set_velocity_2d(
        &mut self,
        entity: u64,
        linear: seal_graph::rt::V2,
        angular: f64,
    ) -> Result<(), String> {
        let id = self.object(entity)?.id.clone();
        self.phys2d.velocity.push((id, [linear.x as f32, linear.y as f32], angular as f32));
        Ok(())
    }

    fn set_collider_shape(
        &mut self,
        entity: u64,
        shape: &str,
        size: seal_graph::rt::V2,
        points: &[[f64; 2]],
    ) -> Result<(), String> {
        let shape = match shape {
            "box" | "circle" | "capsule" | "polygon" => shape,
            other => return Err(format!("unknown collider shape {other}")),
        };
        if !size.x.is_finite() || !size.y.is_finite() {
            return Err("collider size must be a finite number".into());
        }
        if shape == "polygon" && points.len() < 3 {
            return Err("a polygon collider needs at least three points".into());
        }
        if points.iter().any(|p| !p[0].is_finite() || !p[1].is_finite()) {
            return Err("collider points must be finite numbers".into());
        }
        let object = self.object(entity)?;
        if object.phys2d.body.is_empty() && !object.phys2d.part {
            return Err("this object has no 2D collider".into());
        }
        object.phys2d.shape = shape.to_string();
        object.phys2d.size = [finite32(size.x).abs().max(0.001), finite32(size.y).abs().max(0.001)];
        if shape == "polygon" {
            object.phys2d.points =
                points.iter().map(|p| [finite32(p[0]), finite32(p[1])]).collect();
        }
        Ok(())
    }

    fn set_collider_enabled(&mut self, entity: u64, enabled: bool) -> Result<(), String> {
        let object = self.object(entity)?;
        if object.phys2d.body.is_empty() && !object.phys2d.part {
            return Err("this object has no 2D collider".into());
        }
        object.collider_enabled = enabled;
        Ok(())
    }

    fn mass_2d(&mut self, entity: u64) -> f64 {
        let Ok(e) = self.object(entity) else { return 0.0 };
        crate::phys2d::scene_mass(e).into()
    }

    fn set_joint_target_2d(
        &mut self,
        entity: u64,
        target: seal_graph::rt::V2,
    ) -> Result<(), String> {
        let id = self.object(entity)?.id.clone();
        let point = [target.x as f32, target.y as f32];
        self.phys2d.joint_target.push((id.clone(), point));
        if let Some(e) = self.entities.iter_mut().find(|e| e.id == id) {
            e.phys2d.target = point;
        }
        Ok(())
    }

    fn ai_order(&mut self, entity: u64, order: seal_graph::rt::AiOrder) -> Result<(), String> {
        use crate::ai::Goal;
        use seal_graph::rt::AiOrder;
        let p = |v: V3| [v.x as f32, v.y as f32, v.z as f32];
        let goal = match order {
            AiOrder::MoveTo { target, speed } => {
                Goal::MoveTo { target: p(target), speed: speed as f32 }
            }
            AiOrder::Patrol { a, b, speed } => {
                Goal::Patrol { a: p(a), b: p(b), speed: speed as f32 }
            }
            AiOrder::Chase { target, speed } => {
                Goal::Chase { target: self.object(target)?.id.clone(), speed: speed as f32 }
            }
            AiOrder::Stop => Goal::Stop,
        };
        let id = self.object(entity)?.id.clone();
        self.ai_cmds.push((id, goal));
        Ok(())
    }

    fn ai_arrived(&mut self, entity: u64) -> bool {
        match self.object(entity) {
            Ok(e) => {
                let id = e.id.clone();
                self.ai_arrived.contains(&id) && !self.ai_cmds.iter().any(|c| c.0 == id)
            }
            Err(_) => false,
        }
    }

    fn anim(&mut self, entity: u64, anim: Option<&str>) -> Result<(), String> {
        let id = self.object(entity)?.id.clone();
        self.anim_cmds.push((id, anim.map(str::to_string)));
        Ok(())
    }

    fn sequence(&mut self, path: &str) -> Result<(), String> {
        self.anim_cmds.push((String::new(), Some(path.to_string())));
        Ok(())
    }

    fn play_sound(
        &mut self,
        sound: &str,
        volume: f64,
        pitch: f64,
        at: Option<V3>,
    ) -> Result<(), String> {
        if sound.trim().is_empty() {
            return Ok(());
        }
        let at = at.map(|l| [l.x as f32, l.y as f32, l.z as f32]);
        let vol = volume as f32;
        let pit = pitch as f32;
        if crate::sound_container::SoundContainerDef::is_soundset_path(sound) {
            let picks = self.play_container(sound, vol, pit)?;
            for pick in picks {
                self.sounds.push(SoundCue {
                    sound: pick.path,
                    volume: pick.volume,
                    pitch: pick.pitch,
                    at,
                });
            }
        } else {
            self.sounds.push(SoundCue { sound: sound.to_string(), volume: vol, pitch: pit, at });
        }
        Ok(())
    }

    fn play_music(
        &mut self,
        file: &str,
        looping: bool,
        fade: f64,
        loop_start: f64,
    ) -> Result<(), String> {
        if file.trim().is_empty() {
            return Ok(());
        }
        let secs = |v: f64| if v.is_finite() { v.max(0.0) as f32 } else { 0.0 };
        self.music.push(MusicCue::Play {
            file: file.to_string(),
            looping,
            fade: secs(fade),
            loop_start: secs(loop_start),
        });
        Ok(())
    }

    fn stop_music(&mut self, fade: f64) -> Result<(), String> {
        let fade = if fade.is_finite() { fade.max(0.0) as f32 } else { 0.0 };
        self.music.push(MusicCue::Stop { fade });
        Ok(())
    }

    fn set_music_volume(&mut self, volume: f64) -> Result<(), String> {
        if volume.is_finite() {
            self.music.push(MusicCue::Volume { volume: volume.max(0.0) as f32 });
        }
        Ok(())
    }

    fn player(&mut self, index: i64) -> u64 {
        self.game
            .player_index(self.entities, index)
            .map_or(seal_graph::rt::NO_ENTITY, |i| self.handles.handle(self.entities, i))
    }

    fn game(&mut self, cmd: seal_graph::rt::GameCmd) -> Result<(), String> {
        use seal_graph::rt::GameCmd;
        match cmd {
            GameCmd::Possess { index, entity } => {
                if index < 0 {
                    return Err(format!("player {index} does not exist"));
                }
                let id = self.object(entity)?.id.clone();
                self.game.possessed.insert(index, id);
            }
            GameCmd::SpawnPlayer { index } => {
                self.game.spawn_player(self.entities, index)?;
            }
            GameCmd::AddScore { name, amount } => {
                let s = self.game.scores.entry(name).or_insert(0);
                *s = s.saturating_add(amount);
            }
            GameCmd::SetState { state } => self.game.state = state,
            GameCmd::SetSaved { key, value } => {
                self.game.values.insert(key, value);
            }
            GameCmd::SaveGame { ref slot }
            | GameCmd::LoadGame { ref slot }
            | GameCmd::DeleteSave { ref slot } => {
                crate::savegame::slot_name(slot)?;
                self.ui.saves.push(cmd);
            }
            GameCmd::OpenScene { ref path }
            | GameCmd::AddScene { ref path }
            | GameCmd::LoadSceneAsync { ref path, .. }
            | GameCmd::UnloadScene { ref path } => {
                crate::scenes::check(path)?;
                self.ui.scene_cmds.push(cmd);
            }
            GameCmd::KeepAcrossScenes { entity, keep } => {
                let id = self.object(entity)?.id.clone();
                self.ui.keeps.push((id, keep));
            }
            GameCmd::SetWindowMode { mode } => {
                self.game.window_mode = crate::game::normalize_window_mode(&mode)?;
            }
            GameCmd::OpenGameWindow {
                id,
                title,
                width,
                height,
                x,
                y,
                camera_entity,
                ui_screen,
            } => {
                let camera = if camera_entity == 0 || camera_entity == seal_graph::rt::NO_ENTITY {
                    String::new()
                } else {
                    self.object(camera_entity)?.id.clone()
                };
                let spec = crate::extra_window::ExtraWindow {
                    title,
                    width,
                    height,
                    x,
                    y,
                    camera,
                    ui_screen,
                };
                self.game.extra_windows.open_window(id.max(0) as u32, spec)?;
            }
            GameCmd::MoveGameWindow { id, x, y } => {
                self.game.extra_windows.move_window(id, x, y)?;
            }
            GameCmd::ResizeGameWindow { id, width, height } => {
                self.game.extra_windows.resize_window(id, width, height)?;
            }
            GameCmd::CloseGameWindow { id } => {
                self.game.extra_windows.close_window(id)?;
            }
            GameCmd::RequestAttention => {
                self.game.attention_request = true;
            }
            GameCmd::Spectate { entity, on } => {
                self.game.spectate = if on { Some(self.object(entity)?.id.clone()) } else { None };
            }
            GameCmd::Unpossess { index } => {
                self.game.possessed.remove(&index);
            }
        }
        Ok(())
    }

    fn ui_value(&mut self, element: &str) -> f64 {
        self.ui.values.get(element).map_or(0.0, |v| v.0)
    }

    fn ui_input(&mut self, element: &str) -> String {
        self.ui.values.get(element).map(|v| v.1.clone()).unwrap_or_default()
    }

    fn score(&mut self, name: &str) -> i64 {
        self.game.scores.get(name).copied().unwrap_or(0)
    }

    fn game_state(&mut self) -> String {
        self.game.state.clone()
    }

    fn has_save(&mut self, slot: &str) -> bool {
        self.ui.slots.contains(slot.trim())
    }

    fn saved(&mut self, key: &str) -> String {
        self.game.values.get(key).cloned().unwrap_or_default()
    }

    fn scene_loaded(&mut self, path: &str) -> bool {
        let p = crate::scenes::normal(path);
        !p.is_empty() && self.ui.scenes.iter().any(|s| crate::scenes::normal(s) == p)
    }

    fn current_scene(&mut self) -> String {
        self.ui.scenes.first().cloned().unwrap_or_default()
    }

    fn load_data(&mut self, path: &str) -> Result<(), String> {
        let rel = path.trim().replace('\\', "/");
        let Some(root) = self.project else {
            return Err("data needs a project folder".into());
        };
        let full = root.join(&rel);
        if !full.is_file() {
            return Err(format!("{rel} was not found"));
        }
        if !crate::data_asset::is_data_path(&rel) {
            return Err(format!("{rel} is not a data asset"));
        }
        let text = std::fs::read_to_string(&full).map_err(|e| e.to_string())?;
        let type_path = crate::data_asset::data_type_path(&text).map_err(|e| e.to_string())?;
        let type_text = std::fs::read_to_string(root.join(&type_path))
            .map_err(|_| "data type was not found")?;
        let type_def =
            crate::data_asset::DataTypeDef::parse(&type_text).map_err(|e| e.to_string())?;
        crate::data_asset::DataAsset::parse(&text, &type_def).map_err(|e| e.to_string())?;
        Ok(())
    }

    fn data_field(&mut self, path: &str, field: &str) -> String {
        let rel = path.trim().replace('\\', "/");
        let Some(root) = self.project else {
            return String::new();
        };
        let full = root.join(&rel);
        let Ok(text) = std::fs::read_to_string(&full) else {
            return String::new();
        };
        let Ok(type_path) = crate::data_asset::data_type_path(&text) else {
            return String::new();
        };
        let type_full = root.join(&type_path);
        let Ok(type_text) = std::fs::read_to_string(&type_full) else {
            return String::new();
        };
        let Ok(type_def) = crate::data_asset::DataTypeDef::parse(&type_text) else {
            return String::new();
        };
        let Ok(asset) = crate::data_asset::DataAsset::parse(&text, &type_def) else {
            return String::new();
        };
        asset
            .values
            .iter()
            .find(|(name, _)| name == field)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    }

    fn read_csv_cell(&mut self, path: &str, row: i64, col: i64) -> String {
        let Some(root) = self.project else {
            return String::new();
        };
        let full = root.join(path.trim().replace('\\', "/"));
        let Ok(text) = std::fs::read_to_string(&full) else {
            return String::new();
        };
        crate::data_asset::read_csv_cell(&text, row.max(0) as usize, col.max(0) as usize)
            .unwrap_or_default()
    }

    fn read_json_value(&mut self, path: &str, json_path: &str) -> String {
        let Some(root) = self.project else {
            return String::new();
        };
        let full = root.join(path.trim().replace('\\', "/"));
        let Ok(text) = std::fs::read_to_string(&full) else {
            return String::new();
        };
        crate::data_asset::read_json_value(&text, json_path).unwrap_or_default()
    }

    fn utf16_to_utf8(&mut self, encoded: &str) -> String {
        crate::text_encoding::utf16_text_to_utf8(encoded).unwrap_or_else(|e| e)
    }

    fn read_typed_json(&mut self, json: &str, type_path: &str) -> String {
        let Some(root) = self.project else {
            return "open a project first".into();
        };
        let type_full = root.join(type_path.trim().replace('\\', "/"));
        let Ok(type_text) = std::fs::read_to_string(&type_full) else {
            return format!("{type_path} was not found");
        };
        let Ok(type_def) = crate::data_asset::DataTypeDef::parse(&type_text) else {
            return format!("{type_path} is damaged");
        };
        match crate::data_asset::read_typed_json(json, type_path, &type_def) {
            Ok(asset) => {
                asset.values.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("\n")
            }
            Err(e) => e.to_string(),
        }
    }

    fn write_typed_json(&mut self, data_path: &str) -> String {
        let Some(root) = self.project else {
            return "open a project first".into();
        };
        let rel = data_path.trim().replace('\\', "/");
        let full = root.join(&rel);
        let Ok(text) = std::fs::read_to_string(&full) else {
            return format!("{rel} was not found");
        };
        let Ok(type_path) = crate::data_asset::data_type_path(&text) else {
            return "data asset has no type".into();
        };
        let type_full = root.join(&type_path);
        let Ok(type_text) = std::fs::read_to_string(&type_full) else {
            return "data type was not found".into();
        };
        let Ok(type_def) = crate::data_asset::DataTypeDef::parse(&type_text) else {
            return "data type is damaged".into();
        };
        let Ok(asset) = crate::data_asset::DataAsset::parse(&text, &type_def) else {
            return "data asset is damaged".into();
        };
        crate::data_asset::write_typed_json(&asset, &type_def).unwrap_or_else(|e| e.to_string())
    }

    fn unscaled_time(&mut self) -> f64 {
        self.unscaled_time
    }

    fn unscaled_dt(&mut self) -> f64 {
        self.unscaled_dt
    }

    fn sim_tick(&mut self) -> i64 {
        self.sim_tick
    }

    fn project_secret(&mut self) -> String {
        self.project
            .map(|dir| crate::savegame::project_text(dir, "encryption_key"))
            .unwrap_or_default()
    }

    fn open_file_dialog(&mut self, title: &str, filter: &str) -> Result<String, String> {
        Ok(crate::file_dialog::open_file(title, filter))
    }

    fn save_file_dialog(&mut self, title: &str, name: &str) -> Result<String, String> {
        Ok(crate::file_dialog::save_file(title, name))
    }

    fn show_dialog(
        &mut self,
        kind: &str,
        title: &str,
        message: &str,
        placeholder: &str,
        theme: &str,
    ) -> Result<(), String> {
        let kind = crate::dialog_ui::normalize_kind(kind)?.to_string();
        let gen = self.ui.dialog.as_ref().map(|ask| ask.gen).unwrap_or(0).saturating_add(1);
        self.ui.dialog = Some(crate::dialog_ui::Ask {
            gen,
            kind,
            title: title.to_string(),
            message: message.to_string(),
            placeholder: placeholder.to_string(),
            theme: theme.to_string(),
        });
        self.ui.shown.insert(crate::dialog_ui::WIDGET.to_string());
        Ok(())
    }

    fn dialog_button(&mut self) -> String {
        self.ui.dialog_button.clone()
    }

    fn dialog_text(&mut self) -> String {
        self.ui.dialog_text.clone()
    }

    fn cloud_upload(&mut self, slot: &str) -> Result<(), String> {
        crate::savegame::slot_name(slot)?;
        self.ui.cloud.push(crate::cloud::Cmd::Upload(slot.to_string()));
        Ok(())
    }

    fn cloud_download(&mut self, slot: &str) -> Result<(), String> {
        crate::savegame::slot_name(slot)?;
        self.ui.cloud.push(crate::cloud::Cmd::Download(slot.to_string()));
        Ok(())
    }

    fn cloud_flush(&mut self) -> Result<(), String> {
        self.ui.cloud.push(crate::cloud::Cmd::Flush);
        Ok(())
    }

    fn cloud_reply(&mut self, slot: &str, saved_at: i64, body: &str) -> Result<(), String> {
        crate::savegame::slot_name(slot)?;
        self.ui.cloud.push(crate::cloud::Cmd::Reply {
            slot: slot.to_string(),
            saved_at: saved_at.max(0) as u64,
            body: body.to_string(),
        });
        Ok(())
    }

    fn cloud_kind(&mut self) -> String {
        self.ui.cloud_job.as_ref().map(|j| j.kind.clone()).unwrap_or_default()
    }

    fn cloud_slot(&mut self) -> String {
        self.ui.cloud_job.as_ref().map(|j| j.slot.clone()).unwrap_or_default()
    }

    fn cloud_saved_at(&mut self) -> i64 {
        self.ui.cloud_job.as_ref().map(|j| j.saved_at as i64).unwrap_or(0)
    }

    fn cloud_body(&mut self) -> String {
        self.ui.cloud_job.as_ref().map(|j| j.body.clone()).unwrap_or_default()
    }
}

/// The name graphs use for a keyboard key, from the editor's key name:
/// letters stay as they are, digits are spelled out, and the rest follow
/// the long key names (`SpaceBar`, `Left`, `Enter`).
pub fn key_name(key: &str) -> Option<String> {
    const DIGITS: [&str; 10] =
        ["Zero", "One", "Two", "Three", "Four", "Five", "Six", "Seven", "Eight", "Nine"];
    let named = match key {
        "Space" => "SpaceBar",
        // The windowing layer names arrows "Left" or "ArrowLeft" depending on its version.
        "ArrowLeft" | "Left" => "Left",
        "ArrowRight" | "Right" => "Right",
        "ArrowUp" | "Up" => "Up",
        "ArrowDown" | "Down" => "Down",
        "Enter" => "Enter",
        "Escape" => "Escape",
        "Tab" => "Tab",
        "Backspace" => "BackSpace",
        "Delete" => "Delete",
        _ => "",
    };
    if !named.is_empty() {
        return Some(named.into());
    }
    let mut chars = key.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if c.is_ascii_alphabetic() => Some(c.to_ascii_uppercase().to_string()),
        (Some(c), None) if c.is_ascii_digit() => Some(DIGITS[c as usize - '0' as usize].into()),
        _ if key.len() <= 3
            && key.starts_with('F')
            && key[1..].parse::<u8>().is_ok_and(|n| (1..=12).contains(&n)) =>
        {
            Some(key.into())
        }
        _ => None,
    }
}

impl Scripts {
    /// Value of a named input axis from the keys held now.
    pub fn axis_now(&self, name: &str) -> f64 {
        axis_value(name, &self.held)
    }

    /// Whether a key (graph key name) is held now.
    pub fn is_held(&self, key: &str) -> bool {
        self.held.contains(key)
    }
    fn load(&mut self, path: &str) -> Result<Program, String> {
        let project = self.project.clone();
        self.programs
            .entry(path.to_string())
            .or_insert_with(|| {
                let full = project.map_or_else(|| Path::new(path).to_path_buf(), |p| p.join(path));
                std::fs::read_to_string(&full)
                    .map_err(|e| format!("{path}: {e}"))
                    .and_then(|text| Graph::from_text(&text).map_err(|e| format!("{path}: {e}")))
                    .and_then(|g| {
                        Program::build(&g).map_err(|errs| {
                            errs.iter()
                                .map(|e| format!("{path}: {e}"))
                                .collect::<Vec<_>>()
                                .join("; ")
                        })
                    })
            })
            .clone()
    }

    fn name_of(&self, entities: &[Entity], handle: u64) -> String {
        self.handles.index(entities, handle).map(|i| entities[i].name.clone()).unwrap_or_default()
    }

    /// Fire `event` on run `r`; record a failure and return what it spawned.
    fn fire_run(
        &mut self,
        r: usize,
        event: Option<&str>,
        dt: f64,
        entities: &mut Vec<Entity>,
        log: &mut Vec<String>,
    ) -> Vec<u64> {
        self.fire_with(r, event, &[], dt, entities, log)
    }

    fn fire_with(
        &mut self,
        r: usize,
        event: Option<&str>,
        args: &[Value],
        dt: f64,
        entities: &mut Vec<Entity>,
        log: &mut Vec<String>,
    ) -> Vec<u64> {
        if self.runs[r].graph.paused_at().is_some() {
            return Vec::new();
        }
        let run = &mut self.runs[r];
        let mut host = SceneHost {
            entities,
            log,
            handles: &mut self.handles,
            templates: &self.templates,
            spawned: Vec::new(),
            held: &self.held,
            attached: &mut self.attached,
            sounds: &mut self.sounds,
            music: &mut self.music,
            anim_cmds: &mut self.anim_cmds,
            ai_cmds: &mut self.ai_cmds,
            ai_arrived: &self.ai_arrived,
            char_cmds: &mut self.char_cmds,
            char_view: &self.char_view,
            game: &mut self.game,
            ui: &mut self.ui,
            project: self.project.as_deref(),
            container_defs: &mut self.container_defs,
            container_runtimes: &mut self.container_runtimes,
            area_inside: &self.area_inside,
            phys2d: &mut self.phys2d,
            unscaled_time: self.clock_unscaled_time,
            unscaled_dt: self.clock_unscaled_dt,
            sim_tick: self.clock_sim_tick,
            physics: Arc::clone(&self.physics),
        };
        let result = match event {
            Some(name) => run.graph.fire(name, args, &mut host),
            None => run.graph.advance(dt, &mut host),
        };
        let spawned = std::mem::take(&mut host.spawned);
        self.after_run(r, result, entities, log);
        spawned
    }

    /// Record what a run did: the nodes it ran, a breakpoint it stopped at, or its failure.
    fn after_run(
        &mut self,
        r: usize,
        result: Result<RunState, seal_graph::GraphError>,
        entities: &[Entity],
        log: &mut Vec<String>,
    ) {
        let step = self.steps;
        let run = &mut self.runs[r];
        let ran = self.ran.entry(run.path.clone()).or_default();
        for id in run.graph.take_trace() {
            ran.insert(id, step);
        }
        if let Ok(RunState::Paused(node)) = result {
            let (path, handle) = (run.path.clone(), run.object);
            let object = self.name_of(entities, handle);
            log.push(format!("paused at a breakpoint: node {node} in {path} ({object})"));
            self.hit = Some((handle, BreakHit { path, node, object }));
            return;
        }
        let run = &mut self.runs[r];
        if let Err(e) = result {
            run.failed = true;
            let object = run.object;
            let name = self.name_of(entities, object);
            log.push(format!("script on {name}: {e}"));
        }
    }

    /// Continue the run stopped at a breakpoint, all the way or by one
    /// node. Returns whether it is still stopped.
    pub fn debug_continue(
        &mut self,
        single: bool,
        entities: &mut Vec<Entity>,
        log: &mut Vec<String>,
    ) -> bool {
        let Some((handle, hit)) = self.hit.take() else { return false };
        let Some(r) = self.runs.iter().position(|x| x.object == handle && x.path == hit.path)
        else {
            return false;
        };
        let run = &mut self.runs[r];
        let mut host = SceneHost {
            entities,
            log,
            handles: &mut self.handles,
            templates: &self.templates,
            spawned: Vec::new(),
            held: &self.held,
            attached: &mut self.attached,
            sounds: &mut self.sounds,
            music: &mut self.music,
            anim_cmds: &mut self.anim_cmds,
            ai_cmds: &mut self.ai_cmds,
            ai_arrived: &self.ai_arrived,
            char_cmds: &mut self.char_cmds,
            char_view: &self.char_view,
            game: &mut self.game,
            ui: &mut self.ui,
            project: self.project.as_deref(),
            container_defs: &mut self.container_defs,
            container_runtimes: &mut self.container_runtimes,
            area_inside: &self.area_inside,
            phys2d: &mut self.phys2d,
            unscaled_time: self.clock_unscaled_time,
            unscaled_dt: self.clock_unscaled_dt,
            sim_tick: self.clock_sim_tick,
            physics: Arc::clone(&self.physics),
        };
        let result = if single { run.graph.step(&mut host) } else { run.graph.resume(&mut host) };
        let spawned = std::mem::take(&mut host.spawned);
        self.after_run(r, result, entities, log);
        if self.hit.is_none() {
            self.settle(spawned, entities, log);
        }
        self.hit.is_some()
    }

    /// Turn the breakpoints of graph file `path` on in every run of it.
    pub fn set_breakpoints(&mut self, path: &str, nodes: &BTreeSet<u32>) {
        self.breakpoints.insert(path.to_string(), nodes.clone());
        for run in self.runs.iter_mut().filter(|r| r.path == path) {
            let old: Vec<u32> = run.graph.breakpoints().collect();
            for id in old {
                run.graph.set_breakpoint(id, false);
            }
            for &id in nodes {
                run.graph.set_breakpoint(id, true);
            }
        }
    }

    /// What graph file `path` is doing: the stopped run if it is stopped
    /// there, else the first live run of it.
    pub fn debug_view(&self, path: &str, entities: &[Entity]) -> Option<DebugView> {
        let r = match &self.hit {
            Some((handle, h)) if h.path == path => {
                self.runs.iter().position(|x| x.object == *handle && x.path == path)?
            }
            _ => self.runs.iter().position(|r| r.path == path && !r.failed)?,
        };
        let run = self.runs.get(r)?;
        let program = run.graph.program();
        let vars = program
            .vars
            .iter()
            .map(|v| {
                (v.name.clone(), run.graph.var(&v.name).map(|x| x.to_string()).unwrap_or_default())
            })
            .collect();
        let mut values = BTreeMap::new();
        for n in &program.nodes {
            for pin in 0..8u32 {
                if let Some(v) = run.graph.watched(n.id, pin) {
                    values.insert((n.id, pin), v.to_string());
                }
            }
        }
        Some(DebugView {
            paused_at: run.graph.paused_at(),
            call_stack: run.graph.call_stack(),
            ran: self.ran.get(path).cloned().unwrap_or_default(),
            step: self.steps,
            vars,
            values,
            object: self.name_of(entities, run.object),
            state: run.graph.state_name(),
        })
    }

    fn handle_of(&self, entities: &[Entity], object: &str) -> Option<u64> {
        let id = entities.iter().find(|e| e.id == object || e.name == object)?.id.as_str();
        self.handles.ids.iter().find(|(_, stored)| stored.as_str() == id).map(|(handle, _)| *handle)
    }

    /// The live value of one variable on `object`'s running graph.
    pub fn named_var(
        &self,
        entities: &[Entity],
        object: &str,
        name: &str,
    ) -> Option<seal_graph::Value> {
        let handle = self.handle_of(entities, object)?;
        let run = self.runs.iter().find(|run| run.object == handle && !run.failed)?;
        run.graph.var(name).cloned()
    }

    /// Set one variable on `object`'s running graph. The value's type must match.
    pub fn set_named_var(
        &mut self,
        entities: &[Entity],
        object: &str,
        name: &str,
        value: seal_graph::Value,
    ) -> bool {
        let Some(handle) = self.handle_of(entities, object) else { return false };
        let Some(run) = self.runs.iter_mut().find(|run| run.object == handle && !run.failed) else {
            return false;
        };
        run.graph.set_var(name, value).is_ok()
    }

    /// Start the scripts of `objects` and of anything they spawn in turn.
    fn start_objects(
        &mut self,
        mut objects: Vec<u64>,
        entities: &mut Vec<Entity>,
        log: &mut Vec<String>,
    ) {
        while !objects.is_empty() {
            let mut next = Vec::new();
            for handle in objects {
                let Some(i) = self.handles.index(entities, handle) else { continue };
                let path = entities[i].script.trim().to_string();
                if path.is_empty() {
                    continue;
                }
                let program = match self.load(&path) {
                    Ok(p) => p,
                    Err(e) => {
                        log.push(format!("script on {}: {e}", entities[i].name));
                        continue;
                    }
                };
                let has_start = program.events.iter().any(|e| e.name == "start");
                let mut graph = Interp::new(program, handle);
                graph.trace(true);
                graph.watch(true);
                for &id in self.breakpoints.get(&path).into_iter().flatten() {
                    graph.set_breakpoint(id, true);
                }
                self.runs.push(Run { object: handle, path: path.clone(), graph, failed: false });
                if has_start {
                    let r = self.runs.len() - 1;
                    next.extend(self.fire_run(r, Some("start"), 0.0, entities, log));
                }
            }
            objects = next;
        }
    }

    /// Run `event` (or one clock step) on every live graph, then start
    /// what they spawned and remove what they despawned.
    fn each(
        &mut self,
        event: Option<&str>,
        dt: f64,
        entities: &mut Vec<Entity>,
        log: &mut Vec<String>,
        group: Option<&str>,
    ) {
        let mut spawned = Vec::new();
        for r in self.run_order_in(entities, group) {
            let run = &self.runs[r];
            if run.failed || self.handles.index(entities, run.object).is_none() {
                continue;
            }
            spawned.extend(self.fire_run(r, event, dt, entities, log));
        }
        self.settle(spawned, entities, log);
    }

    /// Lower script orders first. Equal orders keep the order the graphs were started in.
    /// `group` keeps only the graphs in that update group.
    fn run_order_in(&self, entities: &[Entity], group: Option<&str>) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.runs.len()).collect();
        if let Some(group) = group {
            order.retain(|&r| {
                let stored = self
                    .handles
                    .index(entities, self.runs[r].object)
                    .map(|i| crate::script_groups::name(&entities[i].script_group))
                    .unwrap_or(crate::script_groups::BEFORE);
                stored == group
            });
        }
        order.sort_by_key(|&r| {
            let rank = self
                .handles
                .index(entities, self.runs[r].object)
                .map(|i| entities[i].script_order)
                .unwrap_or(0);
            (rank, r)
        });
        order
    }

    /// Whether any running graph has an event of this name.
    pub fn listens(&self, event: &str) -> bool {
        self.runs.iter().any(|run| !run.failed && run.graph.program().event(event).is_some())
    }

    /// Fire `event` with one `dt` argument, in script order. Graphs without it are skipped,
    /// so a scene that never uses the event keeps today's step.
    pub fn phase(
        &mut self,
        event: &str,
        dt: f32,
        entities: &mut Vec<Entity>,
        log: &mut Vec<String>,
    ) {
        self.phase_group(event, "", dt, entities, log);
    }

    /// Like [`phase`], but only graphs in `group`. An empty group is every graph.
    pub fn phase_group(
        &mut self,
        event: &str,
        group: &str,
        dt: f32,
        entities: &mut Vec<Entity>,
        log: &mut Vec<String>,
    ) {
        if !self.listens(event) {
            return;
        }
        let args = [Value::Float(f64::from(dt))];
        let mut spawned = Vec::new();
        let only = if group.is_empty() { None } else { Some(group) };
        for r in self.run_order_in(entities, only) {
            if self.runs[r].failed || self.handles.index(entities, self.runs[r].object).is_none() {
                continue;
            }
            if self.runs[r].graph.program().event(event).is_none() {
                continue;
            }
            spawned.extend(self.fire_with(r, Some(event), &args, f64::from(dt), entities, log));
        }
        self.settle(spawned, entities, log);
    }

    /// Fire `event` on the script of object `id`, with the other object as the argument.
    pub fn emit_on(
        &mut self,
        id: &str,
        event: &str,
        other: &str,
        entities: &mut Vec<Entity>,
        log: &mut Vec<String>,
    ) {
        let Some(i) = entities.iter().position(|e| e.id == id) else { return };
        let me = self.handles.handle(entities, i);
        if me == seal_graph::rt::NO_ENTITY {
            return;
        }
        let other_h = entities
            .iter()
            .position(|e| e.id == other)
            .map(|j| self.handles.handle(entities, j))
            .unwrap_or(seal_graph::rt::NO_ENTITY);
        let params = self
            .runs
            .iter()
            .find(|run| run.object == me && !run.failed)
            .and_then(|run| {
                let program = run.graph.program();
                program.event(event).map(|index| program.events[index].params.len())
            })
            .unwrap_or(0);
        let args = if params == 0 { Vec::new() } else { vec![Value::Entity(other_h)] };
        let spawned = self.on(me, event, &args, entities, log);
        self.settle(spawned, entities, log);
    }

    /// Fire `event` on the script of object `id`.
    ///
    /// A graph with one text parameter receives `text`. A graph with no
    /// parameters is still run. Any other parameter list is left alone.
    pub fn emit_text(
        &mut self,
        id: &str,
        event: &str,
        text: &str,
        entities: &mut Vec<Entity>,
        log: &mut Vec<String>,
    ) {
        let Some(i) = entities.iter().position(|e| e.id == id) else { return };
        let me = self.handles.handle(entities, i);
        if me == seal_graph::rt::NO_ENTITY {
            return;
        }
        let mut spawned = Vec::new();
        for r in 0..self.runs.len() {
            if self.runs[r].object != me || self.runs[r].failed {
                continue;
            }
            let args = {
                let Some(index) = self.runs[r].graph.program().event(event) else { continue };
                let kinds: Vec<Ty> = self.runs[r].graph.program().events[index]
                    .params
                    .iter()
                    .map(|(_, ty)| *ty)
                    .collect();
                match kinds.as_slice() {
                    [] => Vec::new(),
                    [Ty::Str] => vec![Value::Str(text.to_string())],
                    _ => continue,
                }
            };
            spawned.extend(self.fire_with(r, Some(event), &args, 0.0, entities, log));
        }
        self.settle(spawned, entities, log);
    }

    /// Remove objects by scene id, the same way a script despawn does.
    pub fn drop_ids(&mut self, ids: &[String], entities: &mut Vec<Entity>, log: &mut Vec<String>) {
        let handles: Vec<u64> = ids
            .iter()
            .filter_map(|id| {
                let i = entities.iter().position(|e| &e.id == id)?;
                let handle = self.handles.handle(entities, i);
                (handle != seal_graph::rt::NO_ENTITY).then_some(handle)
            })
            .collect();
        for handle in handles {
            if !self.handles.dying.contains(&handle) {
                self.handles.dying.push(handle);
            }
        }
        self.settle(Vec::new(), entities, log);
    }

    /// Fire `event` with `args` on the graphs of one object.
    fn on(
        &mut self,
        object: u64,
        event: &str,
        args: &[Value],
        entities: &mut Vec<Entity>,
        log: &mut Vec<String>,
    ) -> Vec<u64> {
        let mut spawned = Vec::new();
        for r in 0..self.runs.len() {
            if self.runs[r].object == object && !self.runs[r].failed {
                spawned.extend(self.fire_with(r, Some(event), args, 0.0, entities, log));
            }
        }
        spawned
    }

    /// Start spawned objects, then give each removed object its
    /// `destroyed` event and take it out of the scene, until nothing is left.
    fn settle(&mut self, mut spawned: Vec<u64>, entities: &mut Vec<Entity>, log: &mut Vec<String>) {
        loop {
            self.start_objects(std::mem::take(&mut spawned), entities, log);
            let Some(gone) = self.handles.dying.first().copied() else { break };
            spawned.extend(self.on(gone, "destroyed", &[], entities, log));
            self.handles.dying.retain(|&h| h != gone);
            if let Some(i) = self.handles.index(entities, gone) {
                let id = entities[i].id.clone();
                self.anim_cmds.retain(|(cmd, _)| cmd != &id);
                entities.remove(i);
            }
            self.handles.ids.remove(&gone);
            self.contacts.retain(|&(a, b)| a != gone && b != gone);
        }
        let handles = &self.handles;
        self.runs.retain(|run| handles.index(entities, run.object).is_some());
    }

    /// Compare which objects overlap now with the last step and fire
    /// `overlap_begin`, `hit` and `overlap_end` on both objects of each pair.
    fn touch(&mut self, entities: &mut Vec<Entity>, log: &mut Vec<String>) {
        let boxes: Vec<TouchBox> = (0..entities.len())
            .filter_map(|i| {
                let e = &entities[i];
                let half = collision_half(e)?;
                Some((
                    self.handles.handle(entities, i),
                    e.translation,
                    half,
                    e.dynamic && e.shape.is_some(),
                ))
            })
            .filter(|b| b.0 != seal_graph::rt::NO_ENTITY)
            .collect();
        let mut now = Vec::new();
        // Solid pairs with where they meet and the normal pointing at the
        // pair's first (lower) handle.
        type Contact = ((u64, u64), [f64; 3], [f64; 3]);
        let mut solid: Vec<Contact> = Vec::new();
        for (i, a) in boxes.iter().enumerate() {
            for b in &boxes[i + 1..] {
                if (0..3).all(|k| (a.1[k] - b.1[k]).abs() < a.2[k] + b.2[k]) {
                    let pair = (a.0.min(b.0), a.0.max(b.0));
                    now.push(pair);
                    if a.3 || b.3 {
                        let (first, second) = if a.0 < b.0 { (a, b) } else { (b, a) };
                        solid.push((
                            pair,
                            contact_point(first, second),
                            contact_normal(first, second),
                        ));
                    }
                }
            }
        }
        let began: Vec<_> = now.iter().copied().filter(|p| !self.contacts.contains(p)).collect();
        let ended: Vec<_> = self.contacts.iter().copied().filter(|p| !now.contains(p)).collect();
        self.contacts = now;
        let mut spawned = Vec::new();
        for (a, b) in began {
            let hit = solid.iter().find(|s| s.0 == (a, b));
            for (me, other, sign) in [(a, b, 1.0), (b, a, -1.0)] {
                spawned.extend(self.on(
                    me,
                    "overlap_begin",
                    &[Value::Entity(other)],
                    entities,
                    log,
                ));
                if let Some(&(_, at, n)) = hit {
                    let v3 =
                        |v: [f64; 3]| Value::Vec3(seal_graph::rt::V3 { x: v[0], y: v[1], z: v[2] });
                    let args = [
                        Value::Entity(other),
                        v3(at),
                        v3(n.map(|c| if c == 0.0 { 0.0 } else { c * sign })),
                    ];
                    spawned.extend(self.on(me, "hit", &args, entities, log));
                }
            }
        }
        for (a, b) in ended {
            for (me, other) in [(a, b), (b, a)] {
                spawned.extend(self.on(me, "overlap_end", &[Value::Entity(other)], entities, log));
            }
        }
        self.settle(spawned, entities, log);
    }

    /// Fire `key_<Key>_pressed` or `key_<Key>_released` on every graph that has it.
    pub fn key(
        &mut self,
        key: &str,
        pressed: bool,
        entities: &mut Vec<Entity>,
        log: &mut Vec<String>,
    ) {
        if pressed {
            self.held.insert(key.to_string());
        } else {
            self.held.remove(key);
        }
        let event = format!("key_{key}_{}", if pressed { "pressed" } else { "released" });
        self.each(Some(&event), 0.0, entities, log, None);
    }

    /// Fire a named event (such as `ui_Play_clicked`) on every object's graph.
    pub fn fire(&mut self, event: &str, entities: &mut Vec<Entity>, log: &mut Vec<String>) {
        self.each(Some(event), 0.0, entities, log, None);
    }

    /// Fire `joint_break` once on each object whose 2D joint snapped.
    pub fn joint_breaks(
        &mut self,
        scene_ids: &[String],
        entities: &mut Vec<Entity>,
        log: &mut Vec<String>,
    ) {
        let mut spawned = Vec::new();
        for scene_id in scene_ids {
            let Some(i) = entities.iter().position(|e| &e.id == scene_id) else { continue };
            let handle = self.handles.handle(entities, i);
            if handle == seal_graph::rt::NO_ENTITY {
                continue;
            }
            spawned.extend(self.on(handle, "joint_break", &[], entities, log));
        }
        self.settle(spawned, entities, log);
    }

    /// Record who is inside each area and fire `body_entered` / `body_exited`
    /// on the area's graphs. `body` is the dynamic body that moved in or out.
    pub fn areas(
        &mut self,
        report: &crate::area_effect::Report,
        entities: &mut Vec<Entity>,
        log: &mut Vec<String>,
    ) {
        self.area_inside = report.inside.clone();
        let mut spawned = Vec::new();
        for (area, body, event) in report
            .entered
            .iter()
            .map(|(a, b)| (a, b, "body_entered"))
            .chain(report.exited.iter().map(|(a, b)| (a, b, "body_exited")))
        {
            let Some(area_i) = entities.iter().position(|e| &e.id == area) else { continue };
            let Some(body_i) = entities.iter().position(|e| &e.id == body) else { continue };
            let area_h = self.handles.handle(entities, area_i);
            let body_h = self.handles.handle(entities, body_i);
            if area_h == seal_graph::rt::NO_ENTITY || body_h == seal_graph::rt::NO_ENTITY {
                continue;
            }
            spawned.extend(self.on(
                area_h,
                event,
                &[seal_graph::Value::Entity(body_h)],
                entities,
                log,
            ));
        }
        self.settle(spawned, entities, log);
    }

    /// Load every object's graph from the project folder and fire `start`.
    /// Returns `None` when no object has a script.
    pub fn start(
        entities: &mut Vec<Entity>,
        project: Option<&Path>,
        breakpoints: &BTreeMap<String, BTreeSet<u32>>,
        log: &mut Vec<String>,
        physics: Arc<Mutex<crate::physics::PhysicsServer>>,
    ) -> Option<Self> {
        if entities.iter().all(|e| e.script.trim().is_empty()) {
            return None;
        }
        let mut templates = HashMap::new();
        for e in entities.iter().filter(|e| !e.source_asset.is_empty()) {
            templates.entry(e.source_asset.clone()).or_insert_with(|| e.clone());
        }
        let mut scripts = Self {
            runs: Vec::new(),
            handles: Handles::default(),
            templates,
            programs: HashMap::new(),
            project: project.map(Path::to_path_buf),
            contacts: Vec::new(),
            container_defs: HashMap::new(),
            container_runtimes: HashMap::new(),
            area_inside: BTreeMap::new(),
            held: Default::default(),
            attached: Vec::new(),
            sounds: Vec::new(),
            music: Vec::new(),
            anim_cmds: Vec::new(),
            ai_cmds: Vec::new(),
            ai_arrived: BTreeSet::new(),
            char_cmds: Vec::new(),
            char_view: BTreeMap::new(),
            phys2d: Phys2dPending::default(),
            physics,
            game: Default::default(),
            ui: GameUi::default(),
            breakpoints: breakpoints.clone(),
            hit: None,
            ran: BTreeMap::new(),
            steps: 0,
            clock_unscaled_time: 0.0,
            clock_unscaled_dt: 0.0,
            clock_sim_tick: 0,
        };
        let all: Vec<u64> = entities.iter().map(|e| scripts.handles.add(e.id.clone())).collect();
        scripts.start_objects(all, entities, log);
        scripts.settle(Vec::new(), entities, log);
        Some(scripts)
    }

    /// Move every attached object to its parent plus its offset, parents
    /// first; attachments whose objects are gone are dropped.
    pub fn follow(&mut self, entities: &mut [Entity]) {
        self.attached.retain(|a| {
            entities.iter().any(|e| e.id == a.child) && entities.iter().any(|e| e.id == a.parent)
        });
        // A chain is at most as long as the list, so that many passes settle it.
        for _ in 0..self.attached.len() {
            for a in &self.attached {
                let Some(at) = entities.iter().find(|e| e.id == a.parent).map(|e| e.translation)
                else {
                    continue;
                };
                if let Some(c) = entities.iter_mut().find(|e| e.id == a.child) {
                    c.translation = [at[0] + a.offset[0], at[1] + a.offset[1], at[2] + a.offset[2]];
                }
            }
        }
    }

    /// Take the objects `remove` out without their `destroyed` event (their
    /// scene went away), then bring `add` in and fire `start` on them.
    pub fn swap_objects(
        &mut self,
        remove: &BTreeSet<String>,
        add: Vec<Entity>,
        entities: &mut Vec<Entity>,
        log: &mut Vec<String>,
    ) {
        let gone: Vec<u64> = self
            .handles
            .ids
            .iter()
            .filter(|(_, id)| remove.contains(*id))
            .map(|(h, _)| *h)
            .collect();
        for h in &gone {
            self.handles.ids.remove(h);
        }
        self.handles.dying.retain(|h| !gone.contains(h));
        self.contacts.retain(|(a, b)| !gone.contains(a) && !gone.contains(b));
        self.runs.retain(|run| !gone.contains(&run.object));
        self.attached.retain(|a| !remove.contains(&a.child) && !remove.contains(&a.parent));
        entities.retain(|e| !remove.contains(&e.id));
        for e in add.iter().filter(|e| !e.source_asset.is_empty()) {
            self.templates.entry(e.source_asset.clone()).or_insert_with(|| e.clone());
        }
        let mut fresh = Vec::new();
        for e in add {
            fresh.push(self.handles.add(e.id.clone()));
            entities.push(e);
        }
        self.start_objects(fresh, entities, log);
        self.settle(Vec::new(), entities, log);
    }

    /// One play step of every running graph, in scene order.
    pub(crate) fn tick_containers(&mut self, dt: f32) {
        for rt in self.container_runtimes.values_mut() {
            rt.tick(dt);
        }
    }

    pub fn set_play_clock(&mut self, unscaled_time: f64, unscaled_dt: f32, sim_tick: u64) {
        self.clock_unscaled_time = unscaled_time;
        self.clock_unscaled_dt = f64::from(unscaled_dt);
        self.clock_sim_tick = sim_tick as i64;
    }

    pub fn step(&mut self, dt: f32, entities: &mut Vec<Entity>, log: &mut Vec<String>) {
        self.tick_containers(dt);
        self.steps += 1;
        self.step_group(crate::script_groups::BEFORE, dt, entities, log);
        self.touch(entities, log);
    }

    /// Frame tick for one update group. Does not advance the clock or the touch pass.
    pub fn step_group(
        &mut self,
        group: &str,
        dt: f32,
        entities: &mut Vec<Entity>,
        log: &mut Vec<String>,
    ) {
        self.each(None, dt as f64, entities, log, Some(group));
    }
}

#[cfg(test)]
mod axis_tests {
    use super::axis_value;

    #[test]
    fn held_keys_push_axes() {
        let held = |keys: &[&str]| keys.iter().map(|k| k.to_string()).collect();
        assert_eq!(axis_value("MoveForward", &held(&["W"])), 1.0);
        assert_eq!(axis_value("MoveForward", &held(&["Down"])), -1.0);
        assert_eq!(axis_value("MoveRight", &held(&["A", "D"])), 0.0);
        assert_eq!(axis_value("MoveUp", &held(&["SpaceBar"])), 1.0);
        assert_eq!(axis_value("Turn", &held(&["W"])), 0.0);
    }
}

#[cfg(test)]
mod key_name_tests {
    #[test]
    fn arrow_keys_name_the_same_in_both_spellings() {
        for (a, b) in [
            ("Right", "ArrowRight"),
            ("Left", "ArrowLeft"),
            ("Up", "ArrowUp"),
            ("Down", "ArrowDown"),
        ] {
            assert_eq!(super::key_name(a), super::key_name(b));
            assert_eq!(super::key_name(a).as_deref(), Some(a));
        }
    }
}
