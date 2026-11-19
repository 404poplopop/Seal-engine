//! Standard scene components.
//!
//! Every tool that opens scene files (the editor, `seal-run`, the agent
//! server and the C interface) registers these, so a scene saved by one
//! opens in all of them. A component equal to its default is not written.
#![forbid(unsafe_code)]

pub mod anim;
pub mod mesh_material;
pub mod property;
pub mod sequence;
pub mod sky_cubemap;
pub mod sprite_shape;
pub mod time_warp;
pub use mesh_material::MeshMaterial;
pub use sky_cubemap::SkyCubemap;
pub use sprite_shape::SpriteShape;

use seal_ecs::{text_component, World};

/// How an entity is drawn.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Look {
    /// `cube`, `sphere`, ... or empty for none.
    pub shape: String,
    /// Red, green and blue, each from 0 to 1.
    pub color: [f32; 3],
    pub roughness: f32,
    pub metal: f32,
}
text_component!(Look as "look" { shape, color, roughness, metal });

/// A flat picture drawn in place of a shape: an image path relative to the
/// project folder, stretched over the entity's x and y scale. With more than
/// one frame the image is a sheet of equal frames left to right, played at
/// `fps` during play. Higher `layer`s draw on top.
#[derive(Clone, Debug, PartialEq)]
pub struct SpriteImage {
    pub image: String,
    pub layer: i32,
    pub frames: u32,
    pub fps: f32,
    /// Clip played from a `.frames` image; empty plays the first clip.
    pub animation: String,
    /// Mirror the picture left-right and top-bottom.
    pub flip_x: bool,
    pub flip_y: bool,
    /// Colour multiplied into the picture: red, green, blue and opacity, each 0 to 1.
    pub tint: [f32; 4],
    /// Draw order among sprites on the same layer: higher draws on top.
    pub order: i32,
    /// Nine-slice borders in picture pixels: left, right, top, bottom. Corners keep
    /// their size and edges stretch when the sprite is scaled; all zero draws it whole.
    pub slice: [f32; 4],
    /// Normal map picture for 2D lights, relative to the project folder; empty for none.
    pub normal_map: String,
    /// How far the sprite moves with the camera, across and up: 1 moves with the world,
    /// 0 stays fixed on screen (a far background), above 1 rushes past (a near foreground).
    pub parallax: [f32; 2],
    /// Repeat the picture side by side so it fills the view across (endless backgrounds).
    pub repeat_x: bool,
    /// Nine-slice edges: `stretch` or `tile`.
    pub slice_mode: String,
    /// World width and height when slicing; zero uses transform scale.
    pub slice_size: [f32; 2],
    /// Pivot inside the picture, 0..1 with y up.
    pub pivot: [f32; 2],
    /// Per-object sort rule name.
    pub sort_mode: String,
}

impl Default for SpriteImage {
    fn default() -> Self {
        Self {
            image: String::new(),
            layer: 0,
            frames: 1,
            fps: 8.0,
            animation: String::new(),
            flip_x: false,
            flip_y: false,
            tint: [1.0; 4],
            order: 0,
            slice: [0.0; 4],
            normal_map: String::new(),
            parallax: [1.0; 2],
            repeat_x: false,
            slice_mode: "stretch".into(),
            slice_size: [0.0; 2],
            pivot: [0.5, 0.5],
            sort_mode: String::new(),
        }
    }
}
text_component!(SpriteImage as "sprite_image" { image, layer, frames, fps, animation, flip_x, flip_y, tint, order, slice, normal_map, parallax, repeat_x, slice_mode, slice_size, pivot, sort_mode });

/// A logic graph that runs on the entity during play: a `.graph` path
/// relative to the project folder.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Script {
    pub graph: String,
}
text_component!(Script as "script" { graph });

/// A particle effect that runs on the entity during play: a `.fx` path
/// relative to the project folder.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Particles {
    pub effect: String,
}
text_component!(Particles as "particles" { effect });

/// Present on entities that move under physics.
#[derive(Clone, Debug, PartialEq)]
pub struct Body {
    pub dynamic: bool,
    /// Surface grip, 0 (ice) and up.
    pub friction: f32,
    /// Bounce, 0 (none) to 1 (full).
    pub restitution: f32,
    /// Walking speed (m/s) of a player-driven character; 0 means not a character.
    pub walk_speed: f32,
}
impl Default for Body {
    fn default() -> Self {
        Self { dynamic: false, friction: 0.6, restitution: 0.05, walk_speed: 0.0 }
    }
}
text_component!(Body as "body" { dynamic, friction, restitution, walk_speed });

/// Light brightness (lux).
#[derive(Clone, Debug, PartialEq)]
pub struct Light {
    pub intensity: f32,
}
impl Default for Light {
    fn default() -> Self {
        Self { intensity: 1.0 }
    }
}
text_component!(Light as "light" { intensity });

/// A light for flat 2D scenes. `kind` is `global`, `point`, `spot`,
/// `directional` or `freeform`. A spot points along the entity's z rotation.
/// A directional light shines parallel to that same angle. A freeform light
/// fills the polygon in `points`.
#[derive(Clone, Debug, PartialEq)]
pub struct Light2d {
    pub kind: String,
    /// Distance where a point or spot light fades to nothing.
    pub range: f32,
    /// Full width of a spot light's cone, in degrees.
    pub cone: f32,
    /// Whether solid objects cast shadows from this light.
    pub shadows: bool,
    /// How this light mixes with the others: `additive`, `multiply` or `alpha`.
    pub blend: String,
    /// Distance outside a freeform polygon where the light fades to nothing.
    pub falloff: f32,
    /// Freeform polygon corners, in the object's own units (x right, y up).
    pub points: Vec<[f32; 2]>,
}
impl Default for Light2d {
    fn default() -> Self {
        Self {
            kind: String::new(),
            range: 5.0,
            cone: 60.0,
            shadows: true,
            blend: "additive".into(),
            falloff: 1.0,
            points: Vec::new(),
        }
    }
}
text_component!(Light2d as "light2d" { kind, range, cone, shadows, blend, falloff, points });

/// A camera for flat 2D play: what it follows, how softly, how far it may go,
/// how close it looks and whether it snaps to the sprite pixel grid.
#[derive(Clone, Debug, PartialEq)]
pub struct Camera2d {
    pub on: bool,
    /// Name of the object to follow; empty follows the player.
    pub follow: String,
    /// Seconds the view takes to catch up (about two thirds of the way); 0 snaps.
    pub smoothing: f32,
    /// How close the view looks: 2 shows things twice as big.
    pub zoom: f32,
    /// Edges the view may not pass: left, right, bottom, top; all zero for none.
    pub limits: [f32; 4],
    /// Sprite pixels per scene unit to snap the view to; 0 turns snapping off.
    pub pixel_snap: f32,
    /// Sorting layers drawn while playing; all bits on by default.
    pub cull_mask: u32,
    /// Dead zone around the view centre, as a fraction of the view:
    /// left, right, top, bottom. 0 keeps the followed target centred.
    pub drag: [f32; 4],
}
impl Default for Camera2d {
    fn default() -> Self {
        Self {
            on: false,
            follow: String::new(),
            smoothing: 0.15,
            zoom: 1.0,
            limits: [0.0; 4],
            pixel_snap: 0.0,
            cull_mask: u32::MAX,
            drag: [0.0; 4],
        }
    }
}
text_component!(Camera2d as "camera2d" { on, follow, smoothing, zoom, limits, pixel_snap, cull_mask, drag });

/// A 2D physics body on a scene object: how it moves, its collision shape,
/// layers, surface, and an optional joint to another object or the world.
#[derive(Clone, Debug, PartialEq)]
pub struct Physics2d {
    /// `dynamic`, `fixed` or `kinematic`; empty means no 2D body.
    pub body: String,
    /// When there is no body, this collider is part of the parent body.
    pub part: bool,
    /// `box`, `circle`, `capsule`, `capsule_x`, `polygon` or `edge`.
    pub shape: String,
    /// Width and height before the object's scale.
    pub size: [f32; 2],
    /// Outline for `polygon` and `edge`, in the object's own units.
    pub points: Vec<[f32; 2]>,
    /// Layer bits the object is on, and the layer bits it collides with.
    pub layer: u32,
    pub mask: u32,
    pub one_way: bool,
    /// Named surface (`ice`, `rubber`, ...); empty uses the object's friction and bounce.
    pub surface: String,
    pub ccd: bool,
    pub density: f32,
    pub mass: f32,
    pub com: [f32; 2],
    pub gravity_scale: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub can_sleep: bool,
    pub lock_rotation: bool,
    pub constant_force: [f32; 2],
    pub constant_torque: f32,
    /// Joint kind; empty for none.
    pub joint: String,
    /// Name of the object the joint holds on to; empty holds on to a point in the world.
    pub joint_to: String,
    pub anchor: [f32; 2],
    pub other_anchor: [f32; 2],
    pub axis: [f32; 2],
    pub limits: [f32; 2],
    pub length: f32,
    pub stiffness: f32,
    pub damping: f32,
    /// Motor speed and the most force it may use; force 0 is no motor.
    pub motor: [f32; 2],
    pub break_force: f32,
    pub break_torque: f32,
    pub max_force: f32,
    pub max_torque: f32,
    pub correction: f32,
    pub offset: [f32; 2],
    pub target: [f32; 2],
}
impl Default for Physics2d {
    fn default() -> Self {
        Self {
            body: String::new(),
            part: false,
            shape: "box".into(),
            size: [1.0, 1.0],
            points: Vec::new(),
            layer: 1,
            mask: u32::MAX,
            one_way: false,
            surface: String::new(),
            ccd: false,
            density: 1.0,
            mass: 0.0,
            com: [0.0; 2],
            gravity_scale: 1.0,
            linear_damping: 0.0,
            angular_damping: 0.0,
            can_sleep: true,
            lock_rotation: false,
            constant_force: [0.0; 2],
            constant_torque: 0.0,
            joint: String::new(),
            joint_to: String::new(),
            anchor: [0.0; 2],
            other_anchor: [0.0; 2],
            axis: [1.0, 0.0],
            limits: [0.0; 2],
            length: 0.0,
            stiffness: 50.0,
            damping: 2.0,
            motor: [0.0; 2],
            break_force: 0.0,
            break_torque: 0.0,
            max_force: 0.0,
            max_torque: 0.0,
            correction: 1.0,
            offset: [0.0; 2],
            target: [0.0; 2],
        }
    }
}
/// A flat 2D walker: finds a way around tiles and solid bodies to its target
/// and steps around other walkers on the way.
#[derive(Clone, Debug, PartialEq)]
pub struct NavAgent2d {
    pub on: bool,
    /// An object's name, or a point as `x, y`; empty stands still.
    pub target: String,
    /// Units per second.
    pub speed: f32,
    /// How far the walker keeps from walls and other walkers.
    pub radius: f32,
    /// Step around other walkers.
    pub avoid: bool,
}
impl Default for NavAgent2d {
    fn default() -> Self {
        Self { on: false, target: String::new(), speed: 3.0, radius: 0.4, avoid: true }
    }
}
text_component!(NavAgent2d as "nav_agent2d" { on, target, speed, radius, avoid });

/// A kinematic 2D body the player or a graph drives, instead of a dynamic body.
/// Off until `on` is set, so scenes that never use it keep their old movement.
#[derive(Clone, Debug, PartialEq)]
pub struct Character2d {
    pub on: bool,
    /// `walk`, `fly`, `swim` or `climb`.
    pub mode: String,
    /// Units per second on a floor, at full input.
    pub speed: f32,
    pub fly_speed: f32,
    pub swim_speed: f32,
    pub climb_speed: f32,
    /// Upward speed of a jump, in units per second.
    pub jump_speed: f32,
    /// Downward acceleration while walking in the air, in units per second squared.
    pub gravity: f32,
    /// Slopes at or under this angle, in degrees, count as a floor.
    pub max_floor_angle: f32,
    /// How high a ledge the body will step onto, in scene units.
    pub step_height: f32,
    /// How far below the feet still counts as a floor, so the body stays on slopes.
    pub floor_snap: f32,
    /// Seconds after leaving a floor during which a jump still counts.
    pub coyote_time: f32,
    /// Seconds a jump press is remembered before the feet find a floor.
    pub jump_buffer: f32,
    /// Full width and height while standing, before the object's scale.
    pub size: [f32; 2],
    /// Full height while crouching. It cannot be taller than `size`'s height.
    pub crouch_height: f32,
    /// Crouch now. Standing back up is refused while a ceiling is in the way.
    pub crouch: bool,
}
impl Default for Character2d {
    fn default() -> Self {
        Self {
            on: false,
            mode: "walk".into(),
            speed: 6.0,
            fly_speed: 8.0,
            swim_speed: 3.0,
            climb_speed: 3.0,
            jump_speed: 12.0,
            gravity: 24.0,
            max_floor_angle: 45.0,
            step_height: 0.4,
            floor_snap: 0.3,
            coyote_time: 0.1,
            jump_buffer: 0.15,
            size: [0.8, 1.6],
            crouch_height: 0.8,
            crouch: false,
        }
    }
}
text_component!(Character2d as "character2d" {
    on, mode, speed, fly_speed, swim_speed, climb_speed, jump_speed, gravity, max_floor_angle,
    step_height, floor_snap, coyote_time, jump_buffer, size, crouch_height, crouch
});

/// A drawn 2D line or filled shape, from points around the object.
#[derive(Clone, Debug, PartialEq)]
pub struct Shape2d {
    /// `line` or `polygon`; empty draws nothing.
    pub kind: String,
    /// Points in the object's own units, before its scale and turn.
    pub points: Vec<[f32; 2]>,
    /// Line thickness in scene units.
    pub width: f32,
    pub color: [f32; 4],
    /// A line that joins its last point back to the first.
    pub closed: bool,
    /// Picture painted across a filled shape. Empty leaves the flat colour.
    pub texture: String,
    /// Picture shift, in widths and heights of the picture.
    pub uv_offset: [f32; 2],
    /// Picture scale around the middle of the shape.
    pub uv_scale: [f32; 2],
    /// Picture turn in degrees, around the middle of the shape.
    pub uv_rotation: f32,
    /// Width along a line, as curve text. Empty keeps `width`.
    pub width_curve: String,
    /// Colour along a line, as gradient text. Empty keeps `color`.
    pub gradient: String,
    /// End of the line at its first point: empty or `none`, `square`, `round`.
    pub cap_begin: String,
    /// End of the line at its last point: empty or `none`, `square`, `round`.
    pub cap_end: String,
}
impl Default for Shape2d {
    fn default() -> Self {
        Self {
            kind: String::new(),
            points: Vec::new(),
            width: 0.1,
            color: [1.0; 4],
            closed: false,
            texture: String::new(),
            uv_offset: [0.0, 0.0],
            uv_scale: [1.0, 1.0],
            uv_rotation: 0.0,
            width_curve: String::new(),
            gradient: String::new(),
            cap_begin: String::new(),
            cap_end: String::new(),
        }
    }
}
text_component!(Shape2d as "shape2d" {
    kind,
    points,
    width,
    color,
    closed,
    texture,
    uv_offset,
    uv_scale,
    uv_rotation,
    width_curve,
    gradient,
    cap_begin,
    cap_end
});

/// Moves the object along another object's line or shape during play.
#[derive(Clone, Debug, PartialEq)]
pub struct PathFollow {
    /// Name of the object whose line is followed; empty is off.
    pub path: String,
    /// Units per second.
    pub speed: f32,
    /// `loop`, `once` or `back` (there and back again).
    pub mode: String,
    /// Turn to face along the path.
    pub rotate: bool,
    /// Distance along the path to start from.
    pub start: f32,
}
impl Default for PathFollow {
    fn default() -> Self {
        Self { path: String::new(), speed: 2.0, mode: "loop".into(), rotate: false, start: 0.0 }
    }
}
text_component!(PathFollow as "path_follow" { path, speed, mode, rotate, start });

/// How a sprite's pixels are drawn: a ready-made effect or a canvas shader file.
#[derive(Clone, Debug, PartialEq)]
pub struct Material2d {
    /// `grayscale`, `outline`, `flash`, `dissolve`, `wave`, `ramp` or `custom`; empty is plain.
    pub effect: String,
    /// Effect colour: the outline, flash, ramp or dissolve edge colour.
    pub color: [f32; 4],
    /// How strong the effect is, 0 to 1.
    pub amount: f32,
    /// Pulses or waves per second; 0 holds the effect still.
    pub speed: f32,
    /// Canvas shader file a `custom` effect runs.
    pub shader: String,
    /// How the sprite combines with what is behind it: `alpha`, `additive`,
    /// `multiply` or `premultiplied`.
    pub blend: String,
    /// `lit`, `unlit` or `light`. `unlit` ignores 2D lights. `light` draws only the light.
    pub light_mode: String,
    /// Horizontal frame count for `particle_anim`.
    pub sheet_cols: u32,
    /// Vertical frame count for `particle_anim`.
    pub sheet_rows: u32,
    /// When true, `particle_anim` loops the sheet; otherwise it holds the last frame.
    pub sheet_loop: bool,
}
impl Default for Material2d {
    fn default() -> Self {
        Self {
            effect: String::new(),
            color: [1.0; 4],
            amount: 1.0,
            speed: 0.0,
            shader: String::new(),
            blend: "alpha".into(),
            light_mode: "lit".into(),
            sheet_cols: 1,
            sheet_rows: 1,
            sheet_loop: true,
        }
    }
}
text_component!(Material2d as "material2d" { effect, color, amount, speed, shader, blend, light_mode, sheet_cols, sheet_rows, sheet_loop });

/// Clips sprites whose sorting layer and order fall in range. `shape` is
/// `rect`, `circle` or `alpha`; empty means this object is not a mask.
/// An alpha mask uses `image` and keeps pixels whose alpha is at least `cutoff`.
#[derive(Clone, Debug, PartialEq)]
pub struct SpriteMask {
    pub shape: String,
    pub image: String,
    pub cutoff: f32,
    pub layer_min: i32,
    pub layer_max: i32,
    pub order_min: i32,
    pub order_max: i32,
}
impl Default for SpriteMask {
    fn default() -> Self {
        Self {
            shape: String::new(),
            image: String::new(),
            cutoff: 0.5,
            layer_min: -1000,
            layer_max: 1000,
            order_min: -1000,
            order_max: 1000,
        }
    }
}
text_component!(SpriteMask as "sprite_mask" {
    shape, image, cutoff, layer_min, layer_max, order_min, order_max
});

/// How a sprite is clipped by sprite masks: `inside` or `outside`.
/// Absent means the sprite is drawn whole.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpriteClip {
    pub mode: String,
}
text_component!(SpriteClip as "sprite_clip" { mode });

text_component!(Physics2d as "physics2d" {
    body, part, shape, size, points, layer, mask, one_way, surface, ccd, density, mass, com,
    gravity_scale, linear_damping, angular_damping, can_sleep, lock_rotation, constant_force,
    constant_torque, joint, joint_to, anchor, other_anchor, axis, limits, length, stiffness,
    damping, motor, break_force, break_torque, max_force, max_torque, correction, offset, target
});

/// Camera lens.
#[derive(Clone, Debug, PartialEq)]
pub struct Camera {
    pub fov: f32,
    pub near: f32,
    pub far: f32,
}
impl Default for Camera {
    fn default() -> Self {
        Self { fov: 60.0, near: 0.1, far: 1000.0 }
    }
}
text_component!(Camera as "camera" { fov, near, far });

/// Where positional sounds are heard from during play.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioListener {
    pub on: bool,
    pub max_distance: f32,
    pub rolloff: f32,
}
impl Default for AudioListener {
    fn default() -> Self {
        Self { on: false, max_distance: 50.0, rolloff: 1.0 }
    }
}
text_component!(AudioListener as "audio_listener" { on, max_distance, rolloff });

/// Soft glow for a flat 2D scene. Off unless `enabled` is set, so older scenes
/// draw exactly as before. `threshold` is picture brightness (1 is full white;
/// higher waits for light that has blown past white). `radius` is in pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct Glow2d {
    pub enabled: bool,
    pub threshold: f32,
    pub intensity: f32,
    pub radius: f32,
}
impl Default for Glow2d {
    fn default() -> Self {
        Self { enabled: false, threshold: 0.8, intensity: 1.0, radius: 4.0 }
    }
}
text_component!(Glow2d as "glow2d" { enabled, threshold, intensity, radius });

/// Editor-only state: the stable key, role tag, visibility and outliner marks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EditorInfo {
    pub key: String,
    pub tag: String,
    pub hidden: bool,
    pub locked: bool,
    pub source_kind: String,
    pub source_asset: String,
    pub unsaved: bool,
    pub play_spawned: bool,
    pub warning: String,
    /// Prefab copies: comma-separated fields this copy changed itself (kept on rebuild).
    pub overrides: String,
}
text_component!(EditorInfo as "editor" {
    key, tag, hidden, locked, source_kind, source_asset, unsaved, play_spawned, warning, overrides
});

/// Register the core components and the standard scene components.
pub fn register_scene(world: &mut World) {
    world.register_core();
    world.register_text::<Look>();
    world.register_text::<Body>();
    world.register_text::<Light>();
    world.register_text::<Light2d>();
    world.register_text::<Camera2d>();
    world.register_text::<Physics2d>();
    world.register_text::<NavAgent2d>();
    world.register_text::<Character2d>();
    world.register_text::<Shape2d>();
    world.register_text::<sprite_shape::SpriteShape>();
    world.register_text::<PathFollow>();
    world.register_text::<Material2d>();
    world.register_text::<SpriteMask>();
    world.register_text::<SpriteClip>();
    world.register_text::<SpriteImage>();
    world.register_text::<Script>();
    world.register_text::<Particles>();
    world.register_text::<Camera>();
    world.register_text::<AudioListener>();
    world.register_text::<EditorInfo>();
    world.register_text::<mesh_material::MeshMaterial>();
    world.register_text_resource::<Glow2d>();
    world.register_text_resource::<sky_cubemap::SkyCubemap>();
}

/// A new world that can read and write standard scenes.
pub fn scene_world() -> World {
    let mut world = World::new();
    register_scene(&mut world);
    world
}
