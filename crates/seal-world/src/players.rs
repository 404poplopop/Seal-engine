//! Spectate, leave a possessed object, and press-start local join.
//!
//! Spectate points the play camera at an object without changing who any
//! player controls. Press Start to Join, when the scene asks for it, gives
//! the next free player index to a gamepad whose Start button just went down.

use std::collections::BTreeMap;

use seal_ecs::text_component;

use crate::{game::Game, Entity};

/// Saved on the scene only while Press Start to Join is on.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LocalJoin {
    pub on: bool,
}

text_component!(LocalJoin as "local_join" { on });

/// Which gamepad belongs to which player while a scene plays. Pad 0 is the
/// first pad. Empty until someone presses Start.
#[derive(Clone, Debug, Default)]
pub struct Roster {
    /// Pad index to player index.
    pub pads: BTreeMap<u32, i64>,
}

/// Give `pad` the next free player index and, when that player has an object
/// tagged `player`, possess it. A pad that already joined is left as it is.
/// Returns the player index when this call assigned the pad.
pub fn join_pad(
    game: &mut Game,
    roster: &mut Roster,
    entities: &[Entity],
    pad: u32,
) -> Option<i64> {
    if roster.pads.contains_key(&pad) {
        return None;
    }
    let index = (0..64).find(|i| !roster.pads.values().any(|n| n == i)).unwrap_or(0);
    roster.pads.insert(pad, index);
    game.devices.insert(pad, index);
    if let Some(body) = entities.iter().filter(|e| e.tag == "player").nth(index as usize) {
        if !game.possessed.values().any(|id| id == &body.id) {
            game.possessed.insert(index, body.id.clone());
        }
    }
    Some(index)
}

/// Drop player `index` so keys no longer treat that object as theirs.
pub fn unpossess(game: &mut Game, index: i64) {
    game.possessed.remove(&index);
}

/// Follow `id` (an object id or name) with the play camera, or stop when `id` is empty.
pub fn spectate(game: &mut Game, id: Option<String>) {
    game.spectate = id.filter(|s| !s.is_empty());
}

/// Flat view: scene point under a screen pointer.
/// `origin` is the screen point of scene (0, 0). `cell` is pixels per unit.
pub fn pointer_world(pointer: [f32; 2], origin: [f32; 2], cell: f32) -> [f32; 2] {
    if !cell.is_finite() || cell.abs() < 1e-4 {
        return [0.0, 0.0];
    }
    [(pointer[0] - origin[0]) / cell, -((pointer[1] - origin[1]) / cell)]
}
