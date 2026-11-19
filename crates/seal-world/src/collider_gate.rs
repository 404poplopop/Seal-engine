//! Whether a 2D collider blocks. On until a script turns it off. The scene
//! file stores it only while it is off, so older scenes stay as they were.

use seal_ecs::text_component;

/// Off means the collider reports nothing and does not push.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColliderGate {
    pub enabled: bool,
}

impl Default for ColliderGate {
    fn default() -> Self {
        Self { enabled: true }
    }
}

text_component!(ColliderGate as "collider_gate" { enabled });
