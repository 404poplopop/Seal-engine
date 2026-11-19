//! When an object's graph runs relative to the physics step, and its order
//! among the other graphs in that same group.
//!
//! An empty group is before physics, which is how scenes that never set one
//! already run. The scene file stores the group only when it is not that.

use seal_ecs::text_component;

/// Runs with the ordinary frame tick, before the physics step.
pub const BEFORE: &str = "before_physics";
/// Runs after the physics step, before the late group.
pub const AFTER: &str = "after_physics";
/// Runs after physics and after the after-physics group.
pub const LATE: &str = "late";

/// Groups the inspector offers, in the order they run.
pub const GROUPS: &[&str] = &[BEFORE, AFTER, LATE];

/// The group a stored value means. Anything else is before physics.
pub fn name(stored: &str) -> &'static str {
    match stored {
        AFTER => AFTER,
        LATE => LATE,
        _ => BEFORE,
    }
}

/// What to keep on the object. Before physics is stored as empty.
pub fn stored(value: &str) -> String {
    match name(value.trim()) {
        AFTER => AFTER.into(),
        LATE => LATE.into(),
        _ => String::new(),
    }
}

/// The group written on an object, from an inspector value.
pub fn apply(group: &mut String, value: &str) {
    let trimmed = value.trim();
    if trimmed.is_empty() || GROUPS.contains(&trimmed) {
        *group = stored(trimmed);
    }
}

/// Scene component. Present only when the group is not before physics.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScriptGroup {
    pub group: String,
}

text_component!(ScriptGroup as "script_group" { group });
