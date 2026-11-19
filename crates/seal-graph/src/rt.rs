//! Runtime pieces shared by the interpreter and by compiled graphs.
//!
//! Both sides call the same functions for every operation, timer and budget
//! step, which is what keeps their results identical.

use std::collections::BTreeMap;
use std::fmt;

/// A 2D vector value.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct V2 {
    pub x: f64,
    pub y: f64,
}

/// A 3D vector value.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct V3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct Quat {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub w: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct Plane {
    pub nx: f64,
    pub ny: f64,
    pub nz: f64,
    pub d: f64,
}

/// Data pin types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Ty {
    Bool,
    Int,
    Float,
    Vec2,
    Str,
    Entity,
    Vec3,
    Quat,
    Plane,
    /// An ordered list of one element type. Lists do not nest.
    List(Elem),
}

/// The element type of a list: any data type except a list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Elem {
    Bool,
    Int,
    Float,
    Vec2,
    Str,
    Entity,
    Vec3,
}

impl Elem {
    pub const ALL: [Elem; 7] =
        [Elem::Bool, Elem::Int, Elem::Float, Elem::Vec2, Elem::Str, Elem::Entity, Elem::Vec3];

    pub fn ty(self) -> Ty {
        match self {
            Elem::Bool => Ty::Bool,
            Elem::Int => Ty::Int,
            Elem::Float => Ty::Float,
            Elem::Vec2 => Ty::Vec2,
            Elem::Str => Ty::Str,
            Elem::Entity => Ty::Entity,
            Elem::Vec3 => Ty::Vec3,
        }
    }

    /// The element type for a data type, if it can be one.
    pub fn of(t: Ty) -> Option<Elem> {
        Elem::ALL.into_iter().find(|e| e.ty() == t)
    }

    /// The `Elem` variant name, for generated code.
    pub fn variant(self) -> &'static str {
        match self {
            Elem::Bool => "Bool",
            Elem::Int => "Int",
            Elem::Float => "Float",
            Elem::Vec2 => "Vec2",
            Elem::Str => "Str",
            Elem::Entity => "Entity",
            Elem::Vec3 => "Vec3",
        }
    }
}

impl Ty {
    pub const ALL: [Ty; 16] = [
        Ty::Bool,
        Ty::Int,
        Ty::Float,
        Ty::Vec2,
        Ty::Str,
        Ty::Entity,
        Ty::Vec3,
        Ty::Quat,
        Ty::Plane,
        Ty::List(Elem::Bool),
        Ty::List(Elem::Int),
        Ty::List(Elem::Float),
        Ty::List(Elem::Vec2),
        Ty::List(Elem::Str),
        Ty::List(Elem::Entity),
        Ty::List(Elem::Vec3),
    ];

    pub fn name(self) -> &'static str {
        match self {
            Ty::Bool => "bool",
            Ty::Int => "int",
            Ty::Float => "float",
            Ty::Vec2 => "vec2",
            Ty::Str => "str",
            Ty::Entity => "entity",
            Ty::Vec3 => "vec3",
            Ty::Quat => "quat",
            Ty::Plane => "plane",
            Ty::List(Elem::Bool) => "list<bool>",
            Ty::List(Elem::Int) => "list<int>",
            Ty::List(Elem::Float) => "list<float>",
            Ty::List(Elem::Vec2) => "list<vec2>",
            Ty::List(Elem::Str) => "list<str>",
            Ty::List(Elem::Entity) => "list<entity>",
            Ty::List(Elem::Vec3) => "list<vec3>",
        }
    }

    pub fn is_list(self) -> bool {
        matches!(self, Ty::List(_))
    }

    pub fn parse(name: &str) -> Option<Ty> {
        Ty::ALL.into_iter().find(|t| t.name() == name)
    }

    /// The value an unset variable, slot or unlinked input starts with.
    pub fn zero(self) -> Value {
        match self {
            Ty::Bool => Value::Bool(false),
            Ty::Int => Value::Int(0),
            Ty::Float => Value::Float(0.0),
            Ty::Vec2 => Value::Vec2(V2 { x: 0.0, y: 0.0 }),
            Ty::Str => Value::Str(String::new()),
            Ty::Entity => Value::Entity(NO_ENTITY),
            Ty::Vec3 => Value::Vec3(V3 { x: 0.0, y: 0.0, z: 0.0 }),
            Ty::Quat => Value::Quat(Quat { x: 0.0, y: 0.0, z: 0.0, w: 1.0 }),
            Ty::Plane => Value::Plane(Plane { nx: 0.0, ny: 1.0, nz: 0.0, d: 0.0 }),
            Ty::List(e) => Value::List(e, Vec::new()),
        }
    }

    /// The Rust type compiled graphs use.
    pub fn rust(self) -> &'static str {
        match self {
            Ty::Bool => "bool",
            Ty::Int => "i64",
            Ty::Float => "f64",
            Ty::Vec2 => "V2",
            Ty::Str => "String",
            Ty::Entity => "u64",
            Ty::Vec3 => "V3",
            Ty::Quat => "Quat",
            Ty::Plane => "Plane",
            Ty::List(_) => "Vec<Value>",
        }
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A data value.
#[derive(Clone, Debug)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Float(f64),
    Vec2(V2),
    Str(String),
    Entity(u64),
    Vec3(V3),
    Quat(Quat),
    Plane(Plane),
    /// A list and its element type, so an empty list still knows its type.
    List(Elem, Vec<Value>),
}

/// Values compare bit for bit, so `0.0` and `-0.0` differ and NaN equals itself.
impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a.to_bits() == b.to_bits(),
            (Value::Vec2(a), Value::Vec2(b)) => {
                a.x.to_bits() == b.x.to_bits() && a.y.to_bits() == b.y.to_bits()
            }
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Entity(a), Value::Entity(b)) => a == b,
            (Value::Vec3(a), Value::Vec3(b)) => {
                a.x.to_bits() == b.x.to_bits()
                    && a.y.to_bits() == b.y.to_bits()
                    && a.z.to_bits() == b.z.to_bits()
            }
            (Value::Quat(a), Value::Quat(b)) => {
                a.x.to_bits() == b.x.to_bits()
                    && a.y.to_bits() == b.y.to_bits()
                    && a.z.to_bits() == b.z.to_bits()
                    && a.w.to_bits() == b.w.to_bits()
            }
            (Value::Plane(a), Value::Plane(b)) => {
                a.nx.to_bits() == b.nx.to_bits()
                    && a.ny.to_bits() == b.ny.to_bits()
                    && a.nz.to_bits() == b.nz.to_bits()
                    && a.d.to_bits() == b.d.to_bits()
            }
            (Value::List(t, a), Value::List(u, b)) => t == u && a == b,
            _ => false,
        }
    }
}

impl Value {
    pub fn ty(&self) -> Ty {
        match self {
            Value::Bool(_) => Ty::Bool,
            Value::Int(_) => Ty::Int,
            Value::Float(_) => Ty::Float,
            Value::Vec2(_) => Ty::Vec2,
            Value::Str(_) => Ty::Str,
            Value::Entity(_) => Ty::Entity,
            Value::Vec3(_) => Ty::Vec3,
            Value::Quat(_) => Ty::Quat,
            Value::Plane(_) => Ty::Plane,
            Value::List(e, _) => Ty::List(*e),
        }
    }

    pub fn bool(&self) -> bool {
        matches!(self, Value::Bool(true))
    }
    pub fn int(&self) -> i64 {
        if let Value::Int(v) = self {
            *v
        } else {
            0
        }
    }
    pub fn float(&self) -> f64 {
        if let Value::Float(v) = self {
            *v
        } else {
            0.0
        }
    }
    pub fn vec2(&self) -> V2 {
        if let Value::Vec2(v) = self {
            *v
        } else {
            V2 { x: 0.0, y: 0.0 }
        }
    }
    pub fn vec3(&self) -> V3 {
        if let Value::Vec3(v) = self {
            *v
        } else {
            V3 { x: 0.0, y: 0.0, z: 0.0 }
        }
    }
    pub fn quat(&self) -> Quat {
        if let Value::Quat(v) = self {
            *v
        } else {
            Quat { x: 0.0, y: 0.0, z: 0.0, w: 1.0 }
        }
    }
    pub fn plane(&self) -> Plane {
        if let Value::Plane(v) = self {
            *v
        } else {
            Plane { nx: 0.0, ny: 1.0, nz: 0.0, d: 0.0 }
        }
    }
    pub fn str(&self) -> String {
        if let Value::Str(v) = self {
            v.clone()
        } else {
            String::new()
        }
    }
    pub fn entity(&self) -> u64 {
        if let Value::Entity(v) = self {
            *v
        } else {
            0
        }
    }
    pub fn list(&self) -> Vec<Value> {
        if let Value::List(_, v) = self {
            v.clone()
        } else {
            Vec::new()
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Bool(v) => write!(f, "{}", ops::bool_str(*v)),
            Value::Int(v) => write!(f, "{}", ops::int_str(*v)),
            Value::Float(v) => write!(f, "{}", ops::float_str(*v)),
            Value::Vec2(v) => write!(f, "{}", ops::vec2_str(*v)),
            Value::Str(v) => write!(f, "{v:?}"),
            Value::Entity(v) => write!(f, "{}", ops::entity_str(*v)),
            Value::Vec3(v) => write!(f, "{}", ops::vec3_str(*v)),
            Value::Quat(v) => write!(f, "Q {:.3} {:.3} {:.3} {:.3}", v.x, v.y, v.z, v.w),
            Value::Plane(v) => write!(f, "P {:.3} {:.3} {:.3} {:.3}", v.nx, v.ny, v.nz, v.d),
            Value::List(_, items) => {
                f.write_str("[")?;
                for (i, v) in items.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{v}")?;
                }
                f.write_str("]")
            }
        }
    }
}

/// A run-time error, tagged with the node id that raised it.
#[derive(Clone, Debug, PartialEq)]
pub struct GraphError {
    pub node: Option<u32>,
    pub message: String,
}

impl GraphError {
    pub fn at(node: u32, message: impl Into<String>) -> Self {
        Self { node: Some(node), message: message.into() }
    }
    pub fn general(message: impl Into<String>) -> Self {
        Self { node: None, message: message.into() }
    }
}

impl fmt::Display for GraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.node {
            Some(n) => write!(f, "node {n}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for GraphError {}

/// What a graph can reach in the world.
pub trait Host {
    /// Read a field. `ty` is the type the graph expects, for hosts whose
    /// storage does not tell int from float.
    fn get(&mut self, entity: u64, component: &str, field: &str, ty: Ty) -> Result<Value, String>;
    fn set(
        &mut self,
        entity: u64,
        component: &str,
        field: &str,
        value: Value,
    ) -> Result<(), String>;
    fn log(&mut self, message: &str);
    /// Create an entity from a prefab by name.
    fn spawn(&mut self, prefab: &str) -> Result<u64, String> {
        Err(format!("this host cannot spawn {prefab}"))
    }
    /// Remove an entity; one that is already gone is not an error.
    fn despawn(&mut self, _entity: u64) -> Result<(), String> {
        Err("this host cannot despawn".into())
    }
    fn exists(&mut self, _entity: u64) -> bool {
        false
    }
    /// Whether `entity` exists and was made from `prefab` or a prefab
    /// based on it (is-a in classic terms).
    fn is_a(&mut self, _entity: u64, _prefab: &str) -> bool {
        false
    }
    /// Every live entity made from `prefab` or a prefab based on it, in
    /// ascending id order so every run sees the same order.
    fn find_all(&mut self, _prefab: &str) -> Vec<u64> {
        Vec::new()
    }
    /// The entity local player `index` controls (0 is the first player),
    /// or no entity.
    fn player(&mut self, _index: i64) -> u64 {
        NO_ENTITY
    }
    /// How far the named input axis is pushed, from -1 to 1; 0 when
    /// nothing drives it.
    fn axis(&mut self, _name: &str) -> f64 {
        0.0
    }
    /// Show or hide an on-screen widget. Hosts without a screen ignore it.
    fn ui_show(&mut self, _widget: &str, _visible: bool) -> Result<(), String> {
        Ok(())
    }
    /// Set an on-screen text element. Hosts without a screen ignore it.
    fn ui_text(&mut self, _element: &str, _text: &str) -> Result<(), String> {
        Ok(())
    }
    /// One entry of a string table. The default returns the key unchanged.
    fn get_string(&mut self, _table: &str, key: &str, _values: &str) -> String {
        key.to_string()
    }
    /// Restart a typewriter, or skip it when `skip` is true.
    fn typewriter(&mut self, _element: &str, _skip: bool) -> Result<(), String> {
        Ok(())
    }
    /// Whether an input action is held. Hosts without input say no.
    fn action_pressed(&mut self, _name: &str) -> bool {
        false
    }
    /// Seconds the action has been held after its modifiers. 0 while it is up.
    fn action_held(&mut self, _name: &str) -> f64 {
        0.0
    }
    /// Whether the action went up this step.
    fn action_released(&mut self, _name: &str) -> bool {
        false
    }
    /// Pointer movement this step, in screen pixels.
    fn mouse_delta(&mut self) -> V2 {
        V2 { x: 0.0, y: 0.0 }
    }
    /// Unaccelerated pointer motion this step. Zero when it was not reported.
    fn raw_mouse_delta(&mut self) -> V2 {
        V2 { x: 0.0, y: 0.0 }
    }
    /// Motion used for mouse look. The default is the cursor motion.
    fn mouse_look(&mut self) -> V2 {
        self.mouse_delta()
    }
    /// Pen pressure, 0 to 1.
    fn pen_pressure(&mut self) -> f64 {
        0.0
    }
    /// Pen tilt, each axis -1 to 1. Zero is upright.
    fn pen_tilt(&mut self) -> V2 {
        V2 { x: 0.0, y: 0.0 }
    }
    /// Whether the eraser end is down.
    fn pen_eraser(&mut self) -> bool {
        false
    }
    /// Gamepad rotation speed, radians per second.
    fn pad_gyro(&mut self) -> V3 {
        V3 { x: 0.0, y: 0.0, z: 0.0 }
    }
    /// Gamepad acceleration, in gravities.
    fn pad_accel(&mut self) -> V3 {
        V3 { x: 0.0, y: 0.0, z: 0.0 }
    }
    /// Why motion sensors are missing, or empty when they were read.
    fn pad_motion_note(&mut self) -> String {
        "motion sensors are not supported on this host".into()
    }
    /// Set the gamepad light bar. Channels are 0 to 1.
    fn set_pad_light(&mut self, _red: f64, _green: f64, _blue: f64) -> Result<(), String> {
        Err("the gamepad light bar is not supported on this host".into())
    }
    /// Mouse wheel this step. Positive is away from the user.
    fn mouse_wheel(&mut self) -> f64 {
        0.0
    }
    /// Pointer position in the flat scene, in scene units.
    fn pointer_world(&mut self) -> V2 {
        V2 { x: 0.0, y: 0.0 }
    }
    /// Rumble the gamepad. Hosts without one ignore it.
    fn rumble(&mut self, _strong: f64, _weak: f64, _seconds: f64) -> Result<(), String> {
        Ok(())
    }
    /// Start waiting for a button to bind to an action. Hosts without input ignore it.
    fn rebind_action(&mut self, _action: &str, _slot: usize) -> Result<(), String> {
        Ok(())
    }
    /// Shake the play view. Hosts without a view ignore it.
    fn camera_shake(&mut self, _strength: f64, _seconds: f64) -> Result<(), String> {
        Ok(())
    }
    /// Nearest object on a ray: (entity, distance). Hosts without a scene find none.
    fn ray_cast(&mut self, _from: V3, _dir: V3, _max: f64) -> Option<(u64, f64)> {
        None
    }
    /// Dynamic bodies inside the trigger area on `entity`, lowest id first.
    fn bodies_inside(&mut self, _entity: u64) -> Vec<u64> {
        Vec::new()
    }
    /// Colliders a physics query can see. Hosts without colliders return none.
    fn query_shapes(&mut self) -> Vec<crate::query::Shape> {
        Vec::new()
    }
    /// Start (`Some(path)`) or stop (`None`) a keyframe animation. Hosts without animation ignore it.
    fn anim(&mut self, _entity: u64, _anim: Option<&str>) -> Result<(), String> {
        Ok(())
    }
    /// Start a whole-scene sequence (.seq path). Hosts without sequences ignore it.
    fn sequence(&mut self, _path: &str) -> Result<(), String> {
        Ok(())
    }
    /// Give an object an AI order. Hosts without AI ignore it.
    fn ai_order(&mut self, _entity: u64, _order: AiOrder) -> Result<(), String> {
        Ok(())
    }
    /// Whether the object's last AI order has finished.
    fn ai_arrived(&mut self, _entity: u64) -> bool {
        false
    }
    /// Change the game rules' state. Hosts without game rules ignore it.
    fn game(&mut self, _cmd: GameCmd) -> Result<(), String> {
        Ok(())
    }
    /// The value of an on-screen control by name; 0 when there is none.
    fn ui_value(&mut self, _element: &str) -> f64 {
        0.0
    }
    /// The text of an on-screen control by name; empty when there is none.
    fn ui_input(&mut self, _element: &str) -> String {
        String::new()
    }
    /// Whether save slot `slot` holds a game. Hosts without saves say no.
    fn has_save(&mut self, _slot: &str) -> bool {
        false
    }
    /// A value kept with `set_saved` (or loaded from a save); empty when there is none.
    fn saved(&mut self, _key: &str) -> String {
        String::new()
    }
    /// Whether the scene file `path` is open or added. Hosts without scenes say no.
    fn scene_loaded(&mut self, _path: &str) -> bool {
        false
    }
    /// The scene file that is open, project-relative; empty when unknown.
    fn current_scene(&mut self) -> String {
        String::new()
    }
    /// Check that a Data asset exists and can be read.
    fn load_data(&mut self, _path: &str) -> Result<(), String> {
        Ok(())
    }
    /// Read one field from a `.data` asset.
    fn data_field(&mut self, _path: &str, _field: &str) -> String {
        String::new()
    }
    /// Read one cell from a CSV file in the project.
    fn read_csv_cell(&mut self, _path: &str, _row: i64, _col: i64) -> String {
        String::new()
    }
    /// Read a value from JSON in the project (`stats.hp` style paths).
    fn read_json_value(&mut self, _path: &str, _json_path: &str) -> String {
        String::new()
    }
    /// Decode comma-separated UTF-16 code units to UTF-8 text.
    fn utf16_to_utf8(&mut self, encoded: &str) -> String {
        if encoded.trim().is_empty() {
            return String::new();
        }
        let units: Option<Vec<u16>> =
            encoded.split(',').map(|part| part.trim().parse::<u16>().ok()).collect();
        units.and_then(|u| String::from_utf16(&u).ok()).unwrap_or_default()
    }
    /// Read JSON text into typed data fields for `type_path`; errors return as text.
    fn read_typed_json(&mut self, _json: &str, _type_path: &str) -> String {
        String::new()
    }
    /// Write a `.data` asset path to JSON; errors return as text.
    fn write_typed_json(&mut self, _data_path: &str) -> String {
        String::new()
    }
    /// Play clock ignoring pause and time scale.
    fn unscaled_time(&mut self) -> f64 {
        0.0
    }
    /// Last raw frame length in seconds.
    fn unscaled_dt(&mut self) -> f64 {
        0.0
    }
    /// Integer play frame counter since play began.
    fn sim_tick(&mut self) -> i64 {
        0
    }
    /// A score by name; 0 when it was never given.
    fn score(&mut self, _name: &str) -> i64 {
        0
    }
    /// The game's state name; empty until a script sets one.
    fn game_state(&mut self) -> String {
        String::new()
    }
    /// Push a dynamic object. Hosts without physics ignore it.
    fn apply_impulse(&mut self, _entity: u64, _impulse: V3) -> Result<(), String> {
        Ok(())
    }
    /// Slide a Character 2D by `wish` this step. Hosts without one ignore it.
    fn character_move(&mut self, _entity: u64, _wish: V2) -> Result<(), String> {
        Ok(())
    }
    /// Ask a Character 2D to jump. Hosts without one ignore it.
    fn character_jump(&mut self, _entity: u64) -> Result<(), String> {
        Ok(())
    }
    /// Set a Character 2D movement mode (`walk`, `fly`, `swim` or `climb`).
    fn character_mode(&mut self, _entity: u64, _mode: &str) -> Result<(), String> {
        Ok(())
    }
    /// Whether a Character 2D stands on a walkable floor.
    fn character_on_floor(&mut self, _entity: u64) -> bool {
        false
    }
    /// Whether a Character 2D is touching a wall.
    fn character_on_wall(&mut self, _entity: u64) -> bool {
        false
    }
    /// Velocity of a Character 2D, in scene units per second.
    fn character_velocity(&mut self, _entity: u64) -> V2 {
        V2 { x: 0.0, y: 0.0 }
    }
    fn apply_force_2d(&mut self, _entity: u64, _force: V2) -> Result<(), String> {
        Ok(())
    }
    fn apply_torque_2d(&mut self, _entity: u64, _torque: f64) -> Result<(), String> {
        Ok(())
    }
    fn set_velocity_2d(&mut self, _entity: u64, _linear: V2, _angular: f64) -> Result<(), String> {
        Ok(())
    }
    /// Replace the 2D collider on `entity`. `points` are polygon corners in the object's units.
    fn set_collider_shape(
        &mut self,
        _entity: u64,
        _shape: &str,
        _size: V2,
        _points: &[[f64; 2]],
    ) -> Result<(), String> {
        Ok(())
    }
    /// Turn the 2D collider on `entity` on or off.
    fn set_collider_enabled(&mut self, _entity: u64, _enabled: bool) -> Result<(), String> {
        Ok(())
    }
    fn set_joint_target_2d(&mut self, _entity: u64, _target: V2) -> Result<(), String> {
        Ok(())
    }
    fn mass_2d(&mut self, _entity: u64) -> f64 {
        0.0
    }
    /// A handle-based physics call. `op` is body, shape, joint, step or position.
    fn physics(&mut self, op: &str, _args: &[Value]) -> Result<Value, String> {
        Err(format!("this host has no physics server ({op})"))
    }
    /// Start playing a sound file once. Hosts without sound ignore it.
    fn play_sound(
        &mut self,
        _sound: &str,
        _volume: f64,
        _pitch: f64,
        _at: Option<V3>,
    ) -> Result<(), String> {
        Ok(())
    }
    /// Start a music file on the music bus. `fade` and `loop_start` are seconds.
    /// Hosts without music ignore it.
    fn play_music(
        &mut self,
        _file: &str,
        _looping: bool,
        _fade: f64,
        _loop_start: f64,
    ) -> Result<(), String> {
        Ok(())
    }
    /// Fade the music bus out over `fade` seconds.
    fn stop_music(&mut self, _fade: f64) -> Result<(), String> {
        Ok(())
    }
    /// Set the music bus volume. 1 is as recorded.
    fn set_music_volume(&mut self, _volume: f64) -> Result<(), String> {
        Ok(())
    }
    /// The project key used to seal saves and text. Empty when none is set.
    fn project_secret(&mut self) -> String {
        String::new()
    }
    /// Seal `text` with the project key.
    fn encrypt_text(&mut self, text: &str) -> Result<String, String> {
        crate::crypto::seal(text, &self.project_secret())
    }
    /// Open text sealed with the project key.
    fn decrypt_text(&mut self, data: &str) -> Result<String, String> {
        crate::crypto::open(data, &self.project_secret())
    }
    /// Ask for an existing file. Empty means cancelled or no dialog.
    fn open_file_dialog(&mut self, _title: &str, _filter: &str) -> Result<String, String> {
        Ok(String::new())
    }
    /// Ask where to write a file. Empty means cancelled or no dialog.
    fn save_file_dialog(&mut self, _title: &str, _name: &str) -> Result<String, String> {
        Ok(String::new())
    }
    /// Show the built-in dialog. `kind` is message, confirm, or input.
    fn show_dialog(
        &mut self,
        _kind: &str,
        _title: &str,
        _message: &str,
        _placeholder: &str,
        _theme: &str,
    ) -> Result<(), String> {
        Ok(())
    }
    /// The button from the last dialog answer: yes, no, or ok.
    fn dialog_button(&mut self) -> String {
        String::new()
    }
    /// The text from the last dialog answer.
    fn dialog_text(&mut self) -> String {
        String::new()
    }
    fn cloud_upload(&mut self, _slot: &str) -> Result<(), String> {
        Ok(())
    }
    fn cloud_download(&mut self, _slot: &str) -> Result<(), String> {
        Ok(())
    }
    fn cloud_flush(&mut self) -> Result<(), String> {
        Ok(())
    }
    fn cloud_reply(&mut self, _slot: &str, _saved_at: i64, _body: &str) -> Result<(), String> {
        Ok(())
    }
    fn cloud_kind(&mut self) -> String {
        String::new()
    }
    fn cloud_slot(&mut self) -> String {
        String::new()
    }
    fn cloud_saved_at(&mut self) -> i64 {
        0
    }
    fn cloud_body(&mut self) -> String {
        String::new()
    }
}

/// The live entity with the lowest id made from `prefab` or one based on it, or the
/// empty entity when there is none.
/// The first state of an instance's random sequence, from its entity.
pub fn rng_seed(me: u64) -> u64 {
    me ^ 0x5EA1_5EED_D1CE_0001
}

/// Steps a random sequence (SplitMix64) and returns the next number.
pub fn rng_next(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// A random whole number from `lo` to `hi`, both included; `lo` when `hi` is smaller.
pub fn random_int(state: &mut u64, lo: i64, hi: i64) -> i64 {
    let r = rng_next(state);
    if hi <= lo {
        return lo;
    }
    let span = (hi as i128 - lo as i128 + 1) as u128;
    (lo as i128 + (r as u128 % span) as i128) as i64
}

/// A random number from `lo` up to `hi`.
pub fn random_float(state: &mut u64, lo: f64, hi: f64) -> f64 {
    let u = (rng_next(state) >> 11) as f64 * (1.0 / (1u64 << 53) as f64);
    lo + (hi - lo) * u
}

pub fn find_first(host: &mut dyn Host, prefab: &str) -> u64 {
    host.find_all(prefab).into_iter().min().unwrap_or(NO_ENTITY)
}

/// Dynamic bodies inside an area, as a list value.
pub fn bodies_inside(host: &mut dyn Host, entity: u64) -> Value {
    let mut ids = host.bodies_inside(entity);
    ids.sort_unstable();
    ids.dedup();
    Value::List(Elem::Entity, ids.into_iter().map(Value::Entity).collect())
}

/// `find_all` as a list value.
pub fn find_all(host: &mut dyn Host, prefab: &str) -> Value {
    let mut ids = host.find_all(prefab);
    ids.sort_unstable();
    ids.dedup();
    Value::List(Elem::Entity, ids.into_iter().map(Value::Entity).collect())
}

/// The empty entity: what an entity variable holds before it is set. No
/// world ever hands it out, so it is never valid.
pub const NO_ENTITY: u64 = u64::MAX;

pub fn spawn(host: &mut dyn Host, prefab: &str, node: u32) -> Result<u64, GraphError> {
    host.spawn(prefab).map_err(|m| GraphError::at(node, m))
}

pub fn despawn(host: &mut dyn Host, entity: u64, node: u32) -> Result<(), GraphError> {
    host.despawn(entity).map_err(|m| GraphError::at(node, m))
}

pub fn sequence(host: &mut dyn Host, path: &str, node: u32) -> Result<(), GraphError> {
    host.sequence(path).map_err(|m| GraphError::at(node, m))
}

pub fn anim(
    host: &mut dyn Host,
    entity: u64,
    anim: Option<&str>,
    node: u32,
) -> Result<(), GraphError> {
    host.anim(entity, anim).map_err(|m| GraphError::at(node, m))
}

/// Which game-rules command a node gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GameKind {
    Possess,
    SpawnPlayer,
    AddScore,
    SetState,
    SaveGame,
    LoadGame,
    DeleteSave,
    SetSaved,
    OpenScene,
    AddScene,
    LoadSceneAsync,
    UnloadScene,
    KeepAcrossScenes,
    SetWindowMode,
    OpenGameWindow,
    MoveGameWindow,
    ResizeGameWindow,
    CloseGameWindow,
    RequestAttention,
    /// Point the play camera at an object without changing who controls it.
    Spectate,
    /// Player `index` controls nothing until possessed again.
    Unpossess,
}

/// A game-rules command: who a player controls, where players start, the
/// score table and the game's state.
#[derive(Clone, Debug, PartialEq)]
pub enum GameCmd {
    /// Player `index` now controls `entity`.
    Possess {
        index: i64,
        entity: u64,
    },
    /// Put player `index`'s object on a start point and stop it.
    SpawnPlayer {
        index: i64,
    },
    AddScore {
        name: String,
        amount: i64,
    },
    SetState {
        state: String,
    },
    /// Write the running game to save slot `slot`.
    SaveGame {
        slot: String,
    },
    /// Put the game saved in `slot` back.
    LoadGame {
        slot: String,
    },
    DeleteSave {
        slot: String,
    },
    /// Keep `value` under `key`; it goes into the next save.
    SetSaved {
        key: String,
        value: String,
    },
    /// Replace the running scene with the scene file `path`; kept objects stay.
    OpenScene {
        path: String,
    },
    /// Add the objects of the scene file `path` to the running scene.
    AddScene {
        path: String,
    },
    /// Read the scene file `path` in the background; it opens (or is added
    /// when `additive`) once it is ready, and `scene_loaded` fires.
    LoadSceneAsync {
        path: String,
        additive: bool,
    },
    /// Take the objects an added scene brought out again.
    UnloadScene {
        path: String,
    },
    /// Whether `entity` (and what is under it) survives `open_scene`.
    KeepAcrossScenes {
        entity: u64,
        keep: bool,
    },
    /// Windowed, fullscreen, or borderless.
    SetWindowMode {
        mode: String,
    },
    OpenGameWindow {
        id: i64,
        title: String,
        width: u32,
        height: u32,
        x: i32,
        y: i32,
        camera_entity: u64,
        ui_screen: String,
    },
    MoveGameWindow {
        id: u32,
        x: i32,
        y: i32,
    },
    ResizeGameWindow {
        id: u32,
        width: u32,
        height: u32,
    },
    CloseGameWindow {
        id: u32,
    },
    RequestAttention,
    /// Follow `entity` with the play camera when `on` is set; otherwise stop.
    Spectate {
        entity: u64,
        on: bool,
    },
    /// Player `index` lets go of the object it controls.
    Unpossess {
        index: i64,
    },
}

/// Which AI order a node gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AiKind {
    MoveTo,
    Chase,
    Patrol,
    Stop,
}

/// An AI order for one object.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AiOrder {
    MoveTo { target: V3, speed: f64 },
    Chase { target: u64, speed: f64 },
    Patrol { a: V3, b: V3, speed: f64 },
    Stop,
}

pub fn ai_order(
    host: &mut dyn Host,
    entity: u64,
    order: AiOrder,
    node: u32,
) -> Result<(), GraphError> {
    host.ai_order(entity, order).map_err(|m| GraphError::at(node, m))
}

pub fn game(host: &mut dyn Host, cmd: GameCmd, node: u32) -> Result<(), GraphError> {
    host.game(cmd).map_err(|m| GraphError::at(node, m))
}

pub fn apply_impulse(
    host: &mut dyn Host,
    entity: u64,
    impulse: V3,
    node: u32,
) -> Result<(), GraphError> {
    host.apply_impulse(entity, impulse).map_err(|m| GraphError::at(node, m))
}

/// Which Character 2D order a node gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CharacterKind {
    Move,
    Jump,
    SetMode,
}

/// What a Character 2D query reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CharacterQuery {
    OnFloor,
    OnWall,
    Velocity,
}

pub fn character_move(
    host: &mut dyn Host,
    entity: u64,
    wish: V2,
    node: u32,
) -> Result<(), GraphError> {
    host.character_move(entity, wish).map_err(|m| GraphError::at(node, m))
}

pub fn character_jump(host: &mut dyn Host, entity: u64, node: u32) -> Result<(), GraphError> {
    host.character_jump(entity).map_err(|m| GraphError::at(node, m))
}

pub fn character_mode(
    host: &mut dyn Host,
    entity: u64,
    mode: &str,
    node: u32,
) -> Result<(), GraphError> {
    host.character_mode(entity, mode).map_err(|m| GraphError::at(node, m))
}

pub fn apply_force_2d(
    host: &mut dyn Host,
    entity: u64,
    force: V2,
    node: u32,
) -> Result<(), GraphError> {
    host.apply_force_2d(entity, force).map_err(|m| GraphError::at(node, m))
}

pub fn apply_torque_2d(
    host: &mut dyn Host,
    entity: u64,
    torque: f64,
    node: u32,
) -> Result<(), GraphError> {
    host.apply_torque_2d(entity, torque).map_err(|m| GraphError::at(node, m))
}

pub fn set_velocity_2d(
    host: &mut dyn Host,
    entity: u64,
    linear: V2,
    angular: f64,
    node: u32,
) -> Result<(), GraphError> {
    host.set_velocity_2d(entity, linear, angular).map_err(|m| GraphError::at(node, m))
}

pub fn set_collider_shape(
    host: &mut dyn Host,
    entity: u64,
    shape: &str,
    size: V2,
    points: &[Value],
    node: u32,
) -> Result<(), GraphError> {
    let points: Vec<[f64; 2]> = points
        .iter()
        .map(|value| {
            let point = value.vec2();
            [point.x, point.y]
        })
        .collect();
    host.set_collider_shape(entity, shape, size, &points).map_err(|m| GraphError::at(node, m))
}

pub fn set_collider_enabled(
    host: &mut dyn Host,
    entity: u64,
    enabled: bool,
    node: u32,
) -> Result<(), GraphError> {
    host.set_collider_enabled(entity, enabled).map_err(|m| GraphError::at(node, m))
}

pub fn set_joint_target_2d(
    host: &mut dyn Host,
    entity: u64,
    target: V2,
    node: u32,
) -> Result<(), GraphError> {
    host.set_joint_target_2d(entity, target).map_err(|m| GraphError::at(node, m))
}

pub fn mass_2d(host: &mut dyn Host, entity: u64) -> f64 {
    host.mass_2d(entity)
}

pub fn physics_server(
    host: &mut dyn Host,
    op: &str,
    args: &[Value],
    node: u32,
) -> Result<Value, GraphError> {
    host.physics(op, args).map_err(|m| GraphError::at(node, m))
}

pub fn load_data(host: &mut dyn Host, path: &str, node: u32) -> Result<(), GraphError> {
    host.load_data(path).map_err(|m| GraphError::at(node, m))
}

/// Start a sound; a host that cannot play sound ignores it.
pub fn play_sound(
    host: &mut dyn Host,
    sound: &str,
    volume: f64,
    pitch: f64,
    at: Option<V3>,
    node: u32,
) -> Result<(), GraphError> {
    host.play_sound(sound, volume.max(0.0), pitch.max(0.01), at)
        .map_err(|m| GraphError::at(node, m))
}

fn seconds(v: f64) -> f64 {
    if v.is_finite() {
        v.max(0.0)
    } else {
        0.0
    }
}

/// Start music on the music bus. An empty file does nothing.
pub fn play_music(
    host: &mut dyn Host,
    file: &str,
    looping: bool,
    fade: f64,
    loop_start: f64,
    node: u32,
) -> Result<(), GraphError> {
    if file.trim().is_empty() {
        return Ok(());
    }
    host.play_music(file, looping, seconds(fade), seconds(loop_start))
        .map_err(|m| GraphError::at(node, m))
}

/// Fade the music bus out.
pub fn stop_music(host: &mut dyn Host, fade: f64, node: u32) -> Result<(), GraphError> {
    host.stop_music(seconds(fade)).map_err(|m| GraphError::at(node, m))
}

/// Set the music bus volume. A non-finite value is ignored.
pub fn set_music_volume(host: &mut dyn Host, volume: f64, node: u32) -> Result<(), GraphError> {
    if !volume.is_finite() {
        return Ok(());
    }
    host.set_music_volume(volume.max(0.0)).map_err(|m| GraphError::at(node, m))
}

pub fn ui_show(
    host: &mut dyn Host,
    widget: &str,
    visible: bool,
    node: u32,
) -> Result<(), GraphError> {
    host.ui_show(widget, visible).map_err(|m| GraphError::at(node, m))
}

/// Set the gamepad light bar. Channels are clamped to 0..1; non-finite
/// channels become 0.
pub fn set_pad_light(
    host: &mut dyn Host,
    red: f64,
    green: f64,
    blue: f64,
    node: u32,
) -> Result<(), GraphError> {
    let channel = |v: f64| {
        if v.is_finite() {
            v.clamp(0.0, 1.0)
        } else {
            0.0
        }
    };
    host.set_pad_light(channel(red), channel(green), channel(blue))
        .map_err(|m| GraphError::at(node, m))
}

/// Rumble the pad; strengths are clamped to 0..1 and non-finite or
/// non-positive lengths do nothing.
pub fn rumble(
    host: &mut dyn Host,
    strong: f64,
    weak: f64,
    seconds: f64,
    node: u32,
) -> Result<(), GraphError> {
    if !(seconds.is_finite() && seconds > 0.0) {
        return Ok(());
    }
    let unit = |v: f64| if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 };
    host.rumble(unit(strong), unit(weak), seconds.min(10.0)).map_err(|m| GraphError::at(node, m))
}

/// Ask the player for a button for `action`; a negative slot is an error.
pub fn rebind_action(
    host: &mut dyn Host,
    action: &str,
    slot: i64,
    node: u32,
) -> Result<(), GraphError> {
    let slot = usize::try_from(slot)
        .map_err(|_| GraphError::at(node, format!("binding slot {slot} is below 0")))?;
    host.rebind_action(action.trim(), slot).map_err(|m| GraphError::at(node, m))
}

/// Shake the play view; negative or non-finite amounts do nothing.
pub fn camera_shake(
    host: &mut dyn Host,
    strength: f64,
    seconds: f64,
    node: u32,
) -> Result<(), GraphError> {
    if !(strength.is_finite() && seconds.is_finite()) || strength <= 0.0 || seconds <= 0.0 {
        return Ok(());
    }
    host.camera_shake(strength, seconds).map_err(|m| GraphError::at(node, m))
}

pub fn ui_text(
    host: &mut dyn Host,
    element: &str,
    text: &str,
    node: u32,
) -> Result<(), GraphError> {
    host.ui_text(element, text).map_err(|m| GraphError::at(node, m))
}

/// A host backed by a plain map, for tests and tools.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MapHost {
    pub fields: BTreeMap<(u64, String, String), Value>,
    pub logs: Vec<String>,
    pub writes: usize,
    /// Fields a spawned entity starts with, by prefab name.
    pub prefabs: BTreeMap<String, Vec<(String, String, Value)>>,
    /// Entities spawned and not yet despawned.
    pub alive: std::collections::BTreeSet<u64>,
    pub spawned: u32,
    /// The prefab each entity was made from (spawned or seeded).
    pub made_from: BTreeMap<u64, String>,
    /// Each prefab's base prefab, if it has one.
    pub bases: BTreeMap<String, String>,
    /// The entity each local player controls, first player first.
    pub players: Vec<u64>,
    /// Input axis values by name; unnamed axes read 0.
    pub axes: BTreeMap<String, f64>,
    /// Sounds started, as `sound volume pitch`.
    pub sounds: Vec<String>,
    /// Music commands: `play file looping fade loop_start`, `stop fade`, `volume v`.
    pub music: Vec<String>,
    /// On-screen widgets shown, and text elements by name.
    pub ui_shown: std::collections::BTreeSet<String>,
    pub ui_texts: BTreeMap<String, String>,
    /// Camera shakes asked for: strength, seconds.
    pub shakes: Vec<(f64, f64)>,
    /// Colliders physics query nodes can see.
    pub shapes: Vec<crate::query::Shape>,
}

impl MapHost {
    pub fn prefab(mut self, name: &str, fields: &[(&str, &str, Value)]) -> Self {
        let f = fields.iter().map(|(c, k, v)| (c.to_string(), k.to_string(), v.clone())).collect();
        self.prefabs.insert(name.into(), f);
        self
    }
    /// Make `name` a prefab based on `base`.
    pub fn based_on(mut self, name: &str, base: &str) -> Self {
        self.bases.insert(name.into(), base.into());
        self
    }
    /// Mark a seeded entity as made from `prefab`.
    pub fn made(mut self, entity: u64, prefab: &str) -> Self {
        self.made_from.insert(entity, prefab.into());
        self
    }
    pub fn with(mut self, entity: u64, component: &str, field: &str, value: Value) -> Self {
        self.fields.insert((entity, component.into(), field.into()), value);
        self
    }
}

impl Host for MapHost {
    fn get(&mut self, entity: u64, component: &str, field: &str, _ty: Ty) -> Result<Value, String> {
        self.fields
            .get(&(entity, component.to_string(), field.to_string()))
            .cloned()
            .ok_or_else(|| format!("{} has no {component}.{field}", ops::entity_str(entity)))
    }
    fn set(
        &mut self,
        entity: u64,
        component: &str,
        field: &str,
        value: Value,
    ) -> Result<(), String> {
        self.writes += 1;
        self.fields.insert((entity, component.into(), field.into()), value);
        Ok(())
    }
    fn log(&mut self, message: &str) {
        self.logs.push(message.to_string());
    }
    fn query_shapes(&mut self) -> Vec<crate::query::Shape> {
        self.shapes.clone()
    }

    fn spawn(&mut self, prefab: &str) -> Result<u64, String> {
        let fields = self.prefabs.get(prefab).ok_or_else(|| format!("no prefab {prefab}"))?.clone();
        // Generation 1 and a high index, so spawned ids never collide with
        // the small hand-picked ids tests seed.
        let e = (1u64 << 32) | (1_000_000 + self.spawned) as u64;
        self.spawned += 1;
        self.alive.insert(e);
        self.made_from.insert(e, prefab.into());
        for (c, f, v) in fields {
            self.fields.insert((e, c, f), v);
        }
        Ok(e)
    }

    fn despawn(&mut self, entity: u64) -> Result<(), String> {
        self.alive.remove(&entity);
        self.made_from.remove(&entity);
        self.fields.retain(|k, _| k.0 != entity);
        Ok(())
    }

    fn exists(&mut self, entity: u64) -> bool {
        self.alive.contains(&entity) || self.fields.keys().any(|k| k.0 == entity)
    }

    fn is_a(&mut self, entity: u64, prefab: &str) -> bool {
        let mut cur = self.made_from.get(&entity);
        // A bounded walk, so a base loop cannot hang a test.
        for _ in 0..=self.bases.len() {
            match cur {
                Some(p) if p == prefab => return true,
                Some(p) => cur = self.bases.get(p),
                None => return false,
            }
        }
        false
    }

    fn player(&mut self, index: i64) -> u64 {
        usize::try_from(index).ok().and_then(|i| self.players.get(i)).copied().unwrap_or(NO_ENTITY)
    }

    fn ui_show(&mut self, widget: &str, visible: bool) -> Result<(), String> {
        if visible {
            self.ui_shown.insert(widget.to_string());
        } else {
            self.ui_shown.remove(widget);
        }
        Ok(())
    }

    fn ui_text(&mut self, element: &str, text: &str) -> Result<(), String> {
        self.ui_texts.insert(element.to_string(), text.to_string());
        Ok(())
    }

    fn camera_shake(&mut self, strength: f64, seconds: f64) -> Result<(), String> {
        self.shakes.push((strength, seconds));
        Ok(())
    }

    fn play_sound(
        &mut self,
        sound: &str,
        volume: f64,
        pitch: f64,
        at: Option<V3>,
    ) -> Result<(), String> {
        match at {
            Some(l) => {
                self.sounds.push(format!("{sound} {volume} {pitch} at {} {} {}", l.x, l.y, l.z))
            }
            None => self.sounds.push(format!("{sound} {volume} {pitch}")),
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
        self.music.push(format!("play {file} {looping} {fade} {loop_start}"));
        Ok(())
    }

    fn stop_music(&mut self, fade: f64) -> Result<(), String> {
        self.music.push(format!("stop {fade}"));
        Ok(())
    }

    fn set_music_volume(&mut self, volume: f64) -> Result<(), String> {
        self.music.push(format!("volume {volume}"));
        Ok(())
    }

    fn axis(&mut self, name: &str) -> f64 {
        self.axes.get(name).copied().unwrap_or(0.0).clamp(-1.0, 1.0)
    }

    fn find_all(&mut self, prefab: &str) -> Vec<u64> {
        let ids: Vec<u64> = self.made_from.keys().copied().collect();
        ids.into_iter().filter(|e| self.exists(*e) && self.is_a(*e, prefab)).collect()
    }
}

/// Read a component field and check its type.
pub fn world_get(
    host: &mut dyn Host,
    entity: u64,
    component: &str,
    field: &str,
    ty: Ty,
    node: u32,
) -> Result<Value, GraphError> {
    let v = host.get(entity, component, field, ty).map_err(|m| GraphError::at(node, m))?;
    if v.ty() != ty {
        return Err(GraphError::at(
            node,
            format!("{component}.{field} is {}, the node expects {ty}", v.ty()),
        ));
    }
    Ok(v)
}

pub fn world_set(
    host: &mut dyn Host,
    entity: u64,
    component: &str,
    field: &str,
    value: Value,
    node: u32,
) -> Result<(), GraphError> {
    host.set(entity, component, field, value).map_err(|m| GraphError::at(node, m))
}

/// A pending piece of work in a run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cont {
    /// Execute the node at this index.
    Exec(u32),
    /// The next iteration of a `for` node.
    Loop { node: u32, next: i64, last: i64 },
    /// The next output of a `sequence` node.
    Seq { node: u32, next: u32 },
    /// Where a `call` continues once the called event ends; a `return`
    /// unwinds to here and fills the call's outputs.
    Ret { node: u32 },
    /// After On Exit of a forced state change: adopt `state`, then run its On Enter.
    Commit { node: u32, state: u32, forced: bool, enter: Option<u32> },
}

/// A delay waiting to resume.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Timer {
    pub wake_at: f64,
    pub seq: u64,
    /// Index of the delay node.
    pub node: u32,
    /// Index of the node to resume at, or of the event an event timer runs.
    pub target: u32,
    /// Event timers: the period of a looping timer (0 = once).
    pub repeat: f64,
    /// True for an event timer (`set_timer`), false for a delay.
    pub event: bool,
    /// Event timers: which setting of the timer this is; see [`Timers::live`].
    pub gen: u64,
}

/// Pending delays, resumed in (wake time, schedule order) order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Timers {
    list: Vec<Timer>,
    seq: u64,
    /// Event -> generation of its current timer. Setting or clearing a
    /// timer moves it on, so firings taken before that are skipped.
    gens: Vec<(u32, u64)>,
    next_gen: u64,
}

/// A looping timer fires at most this many times in one `advance`; the rest
/// of a very long frame is skipped instead of hanging the game.
pub const MAX_CATCH_UP: u32 = 1000;

impl Timers {
    pub fn pending(&self, node: u32) -> bool {
        self.list.iter().any(|t| t.node == node)
    }

    /// Schedule a resume. A delay node that is already waiting ignores the trigger.
    pub fn schedule(&mut self, node: u32, wake_at: f64, target: u32) {
        if self.pending(node) {
            return;
        }
        self.list.push(Timer {
            wake_at,
            seq: self.seq,
            node,
            target,
            repeat: 0.0,
            event: false,
            gen: 0,
        });
        self.seq += 1;
    }

    /// Start event `event`'s timer, replacing a running one. `seconds` of
    /// zero or less (or not a number) just stops it.
    pub fn set_event(&mut self, node: u32, event: u32, now: f64, seconds: f64, looping: bool) {
        self.clear_event(event);
        if seconds.is_nan() || seconds <= 0.0 || seconds.is_infinite() {
            return;
        }
        let gen = self.gens.iter().find(|g| g.0 == event).map(|g| g.1).unwrap_or(0);
        let repeat = if looping { seconds } else { 0.0 };
        self.list.push(Timer {
            wake_at: now + seconds,
            seq: self.seq,
            node,
            target: event,
            repeat,
            event: true,
            gen,
        });
        self.seq += 1;
    }

    /// Stop event `event`'s timer; firings already taken for it are skipped.
    pub fn clear_event(&mut self, event: u32) {
        self.list.retain(|t| !(t.event && t.target == event));
        let gen = self.next_gen;
        self.next_gen += 1;
        match self.gens.iter_mut().find(|g| g.0 == event) {
            Some(g) => g.1 = gen,
            None => self.gens.push((event, gen)),
        }
    }

    /// Whether a firing taken by [`take_due`](Self::take_due) still belongs
    /// to the event's current timer.
    pub fn live(&self, t: &Timer) -> bool {
        !t.event || self.gens.iter().any(|g| g.0 == t.target && g.1 == t.gen)
    }

    /// Remove and return every firing due at `time`, in (wake time, order
    /// set) order. Looping timers come back once per period they cover.
    pub fn take_due(&mut self, time: f64) -> Vec<Timer> {
        let mut due: Vec<Timer> = Vec::new();
        let mut fired: Vec<(u32, u32)> = Vec::new();
        loop {
            let next = self
                .list
                .iter()
                .enumerate()
                .filter(|(_, t)| t.wake_at <= time)
                .min_by(|(_, a), (_, b)| a.wake_at.total_cmp(&b.wake_at).then(a.seq.cmp(&b.seq)))
                .map(|(i, _)| i);
            let Some(i) = next else { break };
            let t = self.list.swap_remove(i);
            due.push(t);
            if t.event && t.repeat > 0.0 {
                let n = match fired.iter_mut().find(|f| f.0 == t.target) {
                    Some(f) => {
                        f.1 += 1;
                        f.1
                    }
                    None => {
                        fired.push((t.target, 1));
                        1
                    }
                };
                let mut wake_at = t.wake_at + t.repeat;
                if n >= MAX_CATCH_UP && wake_at <= time {
                    let skip = ((time - wake_at) / t.repeat).floor() + 1.0;
                    wake_at += skip * t.repeat;
                    // Division rounding can land a hair early; step past `time`.
                    while wake_at <= time {
                        wake_at += t.repeat;
                    }
                }
                self.list.push(Timer { wake_at, seq: self.seq, ..t });
                self.seq += 1;
            }
        }
        due
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }
    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }
    pub fn iter(&self) -> impl Iterator<Item = &Timer> {
        self.list.iter()
    }
    /// Move event timers to new event indices (after a reload); timers of
    /// events that are gone are dropped.
    pub fn remap_events(&mut self, f: impl Fn(u32) -> Option<u32>) {
        self.list = self
            .list
            .iter()
            .filter_map(|t| {
                if t.event {
                    f(t.target).map(|e| Timer { target: e, ..*t })
                } else {
                    Some(*t)
                }
            })
            .collect();
        self.gens = self.gens.iter().filter_map(|&(e, g)| f(e).map(|e| (e, g))).collect();
    }
    pub fn retain_map(&mut self, mut f: impl FnMut(&Timer) -> Option<Timer>) {
        self.list = self.list.iter().filter_map(&mut f).collect();
    }
    pub fn clear(&mut self) {
        self.list.clear();
    }
}

/// How many steps one run may take before it is stopped.
pub const DEFAULT_BUDGET: u64 = 100_000;

/// Counts steps in one run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budget {
    pub left: u64,
}

impl Budget {
    pub fn new(steps: u64) -> Self {
        Self { left: steps }
    }
    pub fn step(&mut self, node: u32) -> Result<(), GraphError> {
        if self.left == 0 {
            return Err(GraphError::at(node, "step budget used up (endless loop?)"));
        }
        self.left -= 1;
        Ok(())
    }
}

/// Whether a run finished or stopped at a breakpoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunState {
    Done,
    /// Paused before the node with this id.
    Paused(u32),
}

/// A running graph: the interpreter or a compiled graph.
pub trait GraphInstance {
    /// Fire an event by name. A graph without that event ignores it.
    fn fire(
        &mut self,
        event: &str,
        args: &[Value],
        host: &mut dyn Host,
    ) -> Result<RunState, GraphError>;
    /// Move time on: resume due delays in order, then fire `tick` with `dt`.
    fn advance(&mut self, dt: f64, host: &mut dyn Host) -> Result<RunState, GraphError>;
    /// Every variable, in declaration order.
    fn vars(&self) -> Vec<(String, Value)>;
    fn time(&self) -> f64;
    fn pending_delays(&self) -> usize;
    fn set_budget(&mut self, steps: u64);
}

/// Check event arguments against the declared parameters.
pub fn check_args(event: &str, params: &[Ty], args: &[Value]) -> Result<(), GraphError> {
    if params.len() != args.len() {
        return Err(GraphError::general(format!(
            "event {event} takes {} arguments, got {}",
            params.len(),
            args.len()
        )));
    }
    for (i, (p, a)) in params.iter().zip(args).enumerate() {
        if a.ty() != *p {
            return Err(GraphError::general(format!(
                "event {event} argument {i} is {p}, got {}",
                a.ty()
            )));
        }
    }
    Ok(())
}

/// Every operation, written once. Integer math wraps and division by zero gives 0.
pub mod ops {
    use super::{Value, V2, V3};

    pub fn int_add(a: i64, b: i64) -> i64 {
        a.wrapping_add(b)
    }
    pub fn int_sub(a: i64, b: i64) -> i64 {
        a.wrapping_sub(b)
    }
    pub fn int_mul(a: i64, b: i64) -> i64 {
        a.wrapping_mul(b)
    }
    pub fn int_div(a: i64, b: i64) -> i64 {
        if b == 0 {
            0
        } else {
            a.wrapping_div(b)
        }
    }
    pub fn int_mod(a: i64, b: i64) -> i64 {
        if b == 0 {
            0
        } else {
            a.wrapping_rem(b)
        }
    }
    /// The remainder of a / b with the sign of a; 0 when |b| is under
    /// 1e-8, as the classic float modulo does.
    pub fn float_mod(a: f64, b: f64) -> f64 {
        if b.abs() <= 1e-8 {
            0.0
        } else {
            a % b
        }
    }
    pub fn int_min(a: i64, b: i64) -> i64 {
        a.min(b)
    }
    pub fn int_max(a: i64, b: i64) -> i64 {
        a.max(b)
    }
    pub fn int_neg(a: i64) -> i64 {
        a.wrapping_neg()
    }
    pub fn int_abs(a: i64) -> i64 {
        a.wrapping_abs()
    }
    pub fn float_add(a: f64, b: f64) -> f64 {
        a + b
    }
    pub fn float_sub(a: f64, b: f64) -> f64 {
        a - b
    }
    pub fn float_mul(a: f64, b: f64) -> f64 {
        a * b
    }
    pub fn float_div(a: f64, b: f64) -> f64 {
        a / b
    }
    pub fn float_min(a: f64, b: f64) -> f64 {
        a.min(b)
    }
    pub fn float_max(a: f64, b: f64) -> f64 {
        a.max(b)
    }
    pub fn float_neg(a: f64) -> f64 {
        -a
    }
    pub fn float_abs(a: f64) -> f64 {
        a.abs()
    }
    pub fn float_sqrt(a: f64) -> f64 {
        a.sqrt()
    }
    pub fn vec2_add(a: V2, b: V2) -> V2 {
        V2 { x: a.x + b.x, y: a.y + b.y }
    }
    pub fn vec2_sub(a: V2, b: V2) -> V2 {
        V2 { x: a.x - b.x, y: a.y - b.y }
    }
    pub fn vec2_neg(a: V2) -> V2 {
        V2 { x: -a.x, y: -a.y }
    }
    pub fn vec2_make(x: f64, y: f64) -> V2 {
        V2 { x, y }
    }
    pub fn vec2_x(v: V2) -> f64 {
        v.x
    }
    pub fn vec2_y(v: V2) -> f64 {
        v.y
    }
    pub fn vec2_scale(v: V2, s: f64) -> V2 {
        V2 { x: v.x * s, y: v.y * s }
    }
    pub fn vec2_length(v: V2) -> f64 {
        (v.x * v.x + v.y * v.y).sqrt()
    }
    pub fn vec2_dot(a: V2, b: V2) -> f64 {
        a.x * b.x + a.y * b.y
    }
    pub fn vec2_normalize(v: V2) -> V2 {
        let l = vec2_length(v);
        if l > 0.0 {
            V2 { x: v.x / l, y: v.y / l }
        } else {
            V2 { x: 0.0, y: 0.0 }
        }
    }
    pub fn vec3_add(a: V3, b: V3) -> V3 {
        V3 { x: a.x + b.x, y: a.y + b.y, z: a.z + b.z }
    }
    pub fn vec3_sub(a: V3, b: V3) -> V3 {
        V3 { x: a.x - b.x, y: a.y - b.y, z: a.z - b.z }
    }
    pub fn vec3_neg(a: V3) -> V3 {
        V3 { x: -a.x, y: -a.y, z: -a.z }
    }
    pub fn vec3_make(x: f64, y: f64, z: f64) -> V3 {
        V3 { x, y, z }
    }
    pub fn vec3_x(v: V3) -> f64 {
        v.x
    }
    pub fn vec3_y(v: V3) -> f64 {
        v.y
    }
    pub fn vec3_z(v: V3) -> f64 {
        v.z
    }
    pub fn vec3_scale(v: V3, s: f64) -> V3 {
        V3 { x: v.x * s, y: v.y * s, z: v.z * s }
    }
    pub fn vec3_dot(a: V3, b: V3) -> f64 {
        a.x * b.x + a.y * b.y + a.z * b.z
    }
    pub fn vec3_length(v: V3) -> f64 {
        vec3_dot(v, v).sqrt()
    }
    pub fn vec3_normalize(v: V3) -> V3 {
        let l = vec3_length(v);
        if l > 0.0 {
            V3 { x: v.x / l, y: v.y / l, z: v.z / l }
        } else {
            V3 { x: 0.0, y: 0.0, z: 0.0 }
        }
    }
    pub fn vec3_cross(a: V3, b: V3) -> V3 {
        V3 { x: a.y * b.z - a.z * b.y, y: a.z * b.x - a.x * b.z, z: a.x * b.y - a.y * b.x }
    }
    pub fn vec3_str(v: V3) -> String {
        format!("({}, {}, {})", v.x, v.y, v.z)
    }
    /// Polynomial sine and cosine from seal-math, so every machine agrees.
    pub fn float_sin(a: f64) -> f64 {
        seal_math::sin_cos_f64(a).0
    }
    pub fn float_cos(a: f64) -> f64 {
        seal_math::sin_cos_f64(a).1
    }
    pub fn float_exp(a: f64) -> f64 {
        seal_math::exp_f64(a)
    }
    pub fn float_ln(a: f64) -> f64 {
        seal_math::ln_f64(a)
    }
    pub fn float_pow(a: f64, b: f64) -> f64 {
        seal_math::pow_f64(a, b)
    }
    pub fn float_atan(a: f64) -> f64 {
        seal_math::atan_f64(a)
    }
    pub fn float_asin(a: f64) -> f64 {
        seal_math::asin_f64(a)
    }
    pub fn float_acos(a: f64) -> f64 {
        seal_math::acos_f64(a)
    }
    pub fn float_atan2(y: f64, x: f64) -> f64 {
        seal_math::atan2_f64(y, x)
    }
    pub fn floor_int(a: f64) -> i64 {
        a.floor() as i64
    }
    pub fn round_int(a: f64) -> i64 {
        a.round() as i64
    }
    pub fn ceil_int(a: f64) -> i64 {
        a.ceil() as i64
    }
    pub fn and(a: bool, b: bool) -> bool {
        a & b
    }
    pub fn or(a: bool, b: bool) -> bool {
        a | b
    }
    pub fn not(a: bool) -> bool {
        !a
    }
    pub fn to_float(a: i64) -> f64 {
        a as f64
    }
    /// Truncates toward zero, saturates at the ends, and turns NaN into 0.
    pub fn to_int(a: f64) -> i64 {
        a as i64
    }
    pub fn concat(a: String, b: String) -> String {
        a + &b
    }
    pub fn str_contains(a: String, b: String) -> bool {
        a.contains(b.as_str())
    }
    pub fn str_starts(a: String, b: String) -> bool {
        a.starts_with(b.as_str())
    }
    pub fn str_ends(a: String, b: String) -> bool {
        a.ends_with(b.as_str())
    }
    pub fn bool_str(v: bool) -> String {
        v.to_string()
    }
    pub fn int_str(v: i64) -> String {
        v.to_string()
    }
    pub fn float_str(v: f64) -> String {
        v.to_string()
    }
    pub fn vec2_str(v: V2) -> String {
        format!("({}, {})", v.x, v.y)
    }
    pub fn str_len(v: String) -> i64 {
        v.chars().count() as i64
    }
    pub fn str_upper(v: String) -> String {
        v.to_uppercase()
    }
    pub fn str_lower(v: String) -> String {
        v.to_lowercase()
    }
    pub fn str_trim(v: String) -> String {
        v.trim().to_string()
    }
    pub fn str_trim_start(v: String) -> String {
        v.trim_start().to_string()
    }
    pub fn str_trim_end(v: String) -> String {
        v.trim_end().to_string()
    }
    /// The sign and digits a text starts with, after leading spaces.
    fn number_prefix(v: &str, fraction: bool) -> &str {
        let t = v.trim_start();
        let b = t.as_bytes();
        let mut i = 0;
        if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
            i += 1;
        }
        let digits = |mut i: usize| {
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            i
        };
        let start = i;
        i = digits(i);
        let mut any = i > start;
        if fraction {
            if i < b.len() && b[i] == b'.' {
                let f = digits(i + 1);
                if any || f > i + 1 {
                    any = true;
                    i = f;
                }
            }
            if any && i < b.len() && (b[i] == b'e' || b[i] == b'E') {
                let mut j = i + 1;
                if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
                    j += 1;
                }
                let e = digits(j);
                if e > j {
                    i = e;
                }
            }
        }
        if any {
            &t[..i]
        } else {
            ""
        }
    }
    pub fn parse_int(v: String) -> i64 {
        let p = number_prefix(&v, false);
        let (neg, d) = match p.as_bytes().first() {
            Some(b'-') => (true, &p[1..]),
            Some(b'+') => (false, &p[1..]),
            _ => (false, p),
        };
        let mut n: i64 = 0;
        for c in d.bytes() {
            let k = (c - b'0') as i64;
            n = if neg {
                n.saturating_mul(10).saturating_sub(k)
            } else {
                n.saturating_mul(10).saturating_add(k)
            };
        }
        n
    }
    pub fn parse_float(v: String) -> f64 {
        number_prefix(&v, true).parse().unwrap_or(0.0)
    }
    /// Digits with a comma before every group of three: `-1,234,567`.
    pub fn int_grouped(v: i64) -> String {
        let d = v.unsigned_abs().to_string();
        let mut out = String::new();
        for (i, c) in d.chars().enumerate() {
            if i > 0 && (d.len() - i).is_multiple_of(3) {
                out.push(',');
            }
            out.push(c);
        }
        if v < 0 {
            out.insert(0, '-');
        }
        out
    }
    /// Six decimals with trailing zeros dropped, keeping one: `5.0`,
    /// `3.00001`, `-0.0`; `inf`, `-inf` and `nan` for the odd values.
    pub fn float_text(v: f64) -> String {
        if v.is_nan() {
            return "nan".into();
        }
        if v.is_infinite() {
            return if v > 0.0 { "inf".into() } else { "-inf".into() };
        }
        let s = format!("{v:.6}");
        let t = s.trim_end_matches('0');
        if t.ends_with('.') {
            format!("{t}0")
        } else {
            t.to_string()
        }
    }
    /// `X=1.000 Y=2.000`: three fixed decimals, as classic vector text.
    pub fn vec2_text(v: V2) -> String {
        format!("X={:.3} Y={:.3}", v.x, v.y)
    }
    /// `X=1.000 Y=2.000 Z=3.000`.
    pub fn vec3_text(v: V3) -> String {
        format!("X={:.3} Y={:.3} Z={:.3}", v.x, v.y, v.z)
    }
    /// At most three decimals, trailing zeros dropped, the whole part
    /// grouped like [`int_grouped`]; `-0` prints as `0`.
    pub fn float_grouped(v: f64) -> String {
        if v.is_nan() {
            return "NaN".into();
        }
        if v.is_infinite() {
            return if v > 0.0 { "inf".into() } else { "-inf".into() };
        }
        let s = format!("{:.3}", v.abs());
        let (whole, frac) = s.split_once('.').unwrap_or((&s, ""));
        let frac = frac.trim_end_matches('0');
        let digits: String = whole.chars().collect();
        let mut out = String::new();
        for (i, c) in digits.chars().enumerate() {
            if i > 0 && (digits.len() - i).is_multiple_of(3) {
                out.push(',');
            }
            out.push(c);
        }
        if !frac.is_empty() {
            out.push('.');
            out.push_str(frac);
        }
        if v < 0.0 && out.bytes().any(|b| (b'1'..=b'9').contains(&b)) {
            out.insert(0, '-');
        }
        out
    }
    pub fn str_str(v: String) -> String {
        v
    }
    /// The `index`v`generation` form entity ids print as.
    pub fn entity_str(v: u64) -> String {
        if v == super::NO_ENTITY {
            return "none".into();
        }
        format!("{}v{}", v as u32, v >> 32)
    }
    /// Lists. An index outside the list reads the element type's zero and
    /// leaves the list unchanged on writes, as the classic array nodes do.
    pub fn list_len(l: Vec<Value>) -> i64 {
        l.len() as i64
    }
    fn at(l: &[Value], i: i64) -> Option<usize> {
        usize::try_from(i).ok().filter(|i| *i < l.len())
    }
    pub fn list_get(l: Vec<Value>, i: i64, zero: Value) -> Value {
        at(&l, i).map_or(zero, |i| l[i].clone())
    }
    pub fn list_push(mut l: Vec<Value>, v: Value) -> Vec<Value> {
        l.push(v);
        l
    }
    pub fn list_set(mut l: Vec<Value>, i: i64, v: Value) -> Vec<Value> {
        if let Some(i) = at(&l, i) {
            l[i] = v;
        }
        l
    }
    pub fn list_insert(mut l: Vec<Value>, v: Value, i: i64) -> Vec<Value> {
        if let Ok(i) = usize::try_from(i) {
            if i <= l.len() {
                l.insert(i, v);
            }
        }
        l
    }
    pub fn list_remove_at(mut l: Vec<Value>, i: i64) -> Vec<Value> {
        if let Some(i) = at(&l, i) {
            l.remove(i);
        }
        l
    }
    pub fn list_remove_item(mut l: Vec<Value>, v: Value) -> Vec<Value> {
        l.retain(|x| *x != v);
        l
    }
    pub fn list_find(l: Vec<Value>, v: Value) -> i64 {
        l.iter().position(|x| *x == v).map_or(-1, |i| i as i64)
    }
    pub fn list_contains(l: Vec<Value>, v: Value) -> bool {
        l.contains(&v)
    }

    pub fn select<T>(c: bool, a: T, b: T) -> T {
        if c {
            a
        } else {
            b
        }
    }
    pub fn lt<T: PartialOrd>(a: T, b: T) -> bool {
        a < b
    }
    pub fn le<T: PartialOrd>(a: T, b: T) -> bool {
        a <= b
    }
    pub fn gt<T: PartialOrd>(a: T, b: T) -> bool {
        a > b
    }
    pub fn ge<T: PartialOrd>(a: T, b: T) -> bool {
        a >= b
    }
    pub fn eq<T: PartialEq>(a: T, b: T) -> bool {
        a == b
    }
    pub fn ne<T: PartialEq>(a: T, b: T) -> bool {
        a != b
    }
}

#[cfg(test)]
mod timer_tests {
    use super::*;

    #[test]
    fn setting_again_replaces_and_clearing_kills_taken_firings() {
        let mut t = Timers::default();
        t.set_event(0, 2, 0.0, 1.0, false);
        t.set_event(0, 2, 0.0, 3.0, false);
        assert!(t.take_due(2.0).is_empty());
        let due = t.take_due(3.0);
        assert_eq!(due.len(), 1);
        assert!(t.live(&due[0]));
        t.set_event(0, 2, 3.0, 1.0, true);
        let due = t.take_due(5.5);
        assert_eq!(due.len(), 2);
        t.clear_event(2);
        assert!(!t.live(&due[1]), "a firing taken before a clear is skipped");
        assert!(t.take_due(100.0).is_empty());
    }

    #[test]
    fn looping_catch_up_is_capped() {
        let mut t = Timers::default();
        t.set_event(0, 0, 0.0, 0.001, true);
        assert_eq!(t.take_due(1e6).len(), MAX_CATCH_UP as usize);
        assert_eq!(t.take_due(1e6).len(), 0, "nothing left due after skipping ahead");
        assert_eq!(t.take_due(1e6 + 0.001).len(), 1, "the loop resumes within one period");
    }

    #[test]
    fn bad_seconds_just_stop() {
        let mut t = Timers::default();
        t.set_event(0, 1, 0.0, 1.0, true);
        t.set_event(0, 1, 0.0, f64::NAN, true);
        t.set_event(0, 3, 0.0, -1.0, true);
        assert!(t.take_due(10.0).is_empty());
    }
}
