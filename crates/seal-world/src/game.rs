//! Game rules for a playing scene: which object each player controls, where
//! players start, named scores and the game's state. Everything here lives
//! only while the scene plays; stop throws it away with the rest of play.

use crate::Entity;
use crate::extra_window::ExtraWindowTable;
use std::collections::BTreeMap;

/// The tag that marks a start point for players.
pub const PLAYER_START: &str = "player_start";

/// `windowed`, `fullscreen`, or `borderless`.
pub fn normalize_window_mode(mode: &str) -> Result<String, String> {
    match mode.trim().to_ascii_lowercase().as_str() {
        "windowed" | "window" => Ok("windowed".into()),
        "fullscreen" | "full screen" => Ok("fullscreen".into()),
        "borderless" => Ok("borderless".into()),
        other => Err(format!("window mode is windowed, fullscreen, or borderless, not {other}")),
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Game {
    /// Object id each player index controls, set by `possess`.
    pub possessed: BTreeMap<i64, String>,
    pub scores: BTreeMap<String, i64>,
    /// Empty until a script sets it.
    pub state: String,
    /// Windowed, fullscreen, or borderless, when a script set one. Empty keeps the project setting.
    pub window_mode: String,
    /// Values scripts kept with `set_saved`; they go into saves.
    pub values: BTreeMap<String, String>,
    /// Object the play camera follows without changing who a player controls.
    pub spectate: Option<String>,
    /// Last checkpoint, in scene units. A kill zone that respawns uses it.
    pub checkpoint: Option<[f32; 3]>,
    /// Gamepad index to player index, filled by Press Start to Join.
    pub devices: BTreeMap<u32, i64>,
    /// Extra OS windows while play runs.
    pub extra_windows: ExtraWindowTable,
    /// Set by a graph node; the shell flashes the taskbar once and clears this.
    pub attention_request: bool,
}

impl Game {
    /// The object player `index` controls: the one it possessed if that still
    /// exists, else the `index`-th object tagged `player`.
    pub fn player_index(&self, entities: &[Entity], index: i64) -> Option<usize> {
        if let Some(id) = self.possessed.get(&index) {
            if let Some(i) = entities.iter().position(|e| &e.id == id) {
                return Some(i);
            }
        }
        let n = usize::try_from(index).ok()?;
        entities.iter().enumerate().filter(|(_, e)| e.tag == "player").map(|(i, _)| i).nth(n)
    }

    /// Put player `index`'s object on start point `index` (wrapping round when
    /// there are fewer starts than players) and stop it moving. Returns the
    /// object's index, or why it could not.
    pub fn spawn_player(&self, entities: &mut [Entity], index: i64) -> Result<usize, String> {
        let me = self
            .player_index(entities, index)
            .ok_or_else(|| format!("player {index} controls no object"))?;
        let starts: Vec<usize> = entities
            .iter()
            .enumerate()
            .filter(|(_, e)| e.tag == PLAYER_START)
            .map(|(i, _)| i)
            .collect();
        if starts.is_empty() {
            return Err(format!("no object is tagged {PLAYER_START}"));
        }
        let s = starts[index.rem_euclid(starts.len() as i64) as usize];
        let (at, turn) = (entities[s].translation, entities[s].rotation);
        let e = &mut entities[me];
        e.translation = at;
        e.rotation = turn;
        e.velocity = [0.0; 3];
        Ok(me)
    }

    /// One line for the play bar: the state and the scores, or None when a
    /// script set neither.
    pub fn summary(&self) -> Option<String> {
        let mut parts = Vec::new();
        if !self.state.is_empty() {
            parts.push(self.state.clone());
        }
        parts.extend(self.scores.iter().map(|(k, v)| format!("{k} {v}")));
        (!parts.is_empty()).then(|| parts.join(" · "))
    }
}
