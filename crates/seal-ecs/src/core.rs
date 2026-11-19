//! Components every Seal world understands.

use crate::text_component;
use crate::World;

/// A display name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Name(pub String);
text_component!(Name as "name" newtype);

/// Position (metres), rotation (degrees, pitch/yaw/roll) and scale, relative to the parent.
#[derive(Clone, Debug, PartialEq)]
pub struct Transform {
    pub translation: [f32; 3],
    pub rotation: [f32; 3],
    pub scale: [f32; 3],
}

impl Default for Transform {
    fn default() -> Self {
        Self { translation: [0.0; 3], rotation: [0.0; 3], scale: [1.0; 3] }
    }
}
text_component!(Transform as "transform" { translation, rotation, scale });

/// Scene-wide settings, saved in the scene file's `resources` block.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneSettings {
    /// Downward pull along y while play runs, in units per second squared.
    pub gravity: f32,
    /// Multiplier on the simulation step. Zero pauses motion.
    pub time_scale: f32,
    /// Project-relative sound file played on the music bus when play begins. Empty plays nothing.
    pub music: String,
}

impl Default for SceneSettings {
    fn default() -> Self {
        Self { gravity: 9.81, time_scale: 1.0, music: String::new() }
    }
}
text_component!(SceneSettings as "scene_settings" { gravity, time_scale, music });

impl World {
    /// Register the core components (name, transform) and the core
    /// resource (scene settings) for text and snapshots.
    pub fn register_core(&mut self) {
        self.register_text::<Name>();
        self.register_text::<Transform>();
        self.register_text_resource::<SceneSettings>();
    }
}
