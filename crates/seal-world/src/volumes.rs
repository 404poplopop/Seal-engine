//! Gameplay volumes: kill, damage, checkpoint, and trigger.
//!
//! Create > Areas adds one already filled in. The inspector edits the same
//! fields the scene file stores. An empty kind is not a volume, and it is not
//! written, so scenes that never use one stay as they were.

use std::collections::{BTreeMap, BTreeSet};

use seal_ecs::text_component;

use crate::{game::Game, Entity};

/// Kinds the inspector offers. `none` clears the volume.
pub const KINDS: &[&str] = &["none", "kill", "damage", "checkpoint", "trigger", "post"];
/// What a kill zone does to a player who enters it.
pub const KILL_MODES: &[&str] = &["respawn", "destroy"];

/// Inspector fields, in order. Rows for a kind are hidden until that kind is on.
pub const KEYS: &[&str] =
    &["volume_kind", "volume_mode", "volume_variable", "volume_rate", "volume_event", "volume_tag"];

/// A gameplay volume on one object. An empty `kind` means there is none.
#[derive(Clone, Debug, PartialEq)]
pub struct Volume {
    /// `kill`, `damage`, `checkpoint`, `trigger`, or empty.
    pub kind: String,
    /// Kill zones: `respawn` or `destroy`.
    pub mode: String,
    /// Score or kept value damaged each second. Empty uses `health`.
    pub variable: String,
    /// Points removed per second while a player is inside a damage zone.
    pub rate: f32,
    /// Event fired on the volume. Empty uses the kind's own name.
    pub event: String,
    /// Trigger volumes: only objects with this tag. Empty allows every tag.
    /// Several tags are separated by commas.
    pub tag: String,
}

impl Default for Volume {
    fn default() -> Self {
        Self {
            kind: String::new(),
            mode: "respawn".into(),
            variable: "health".into(),
            rate: 10.0,
            event: String::new(),
            tag: String::new(),
        }
    }
}

text_component!(Volume as "volume" { kind, mode, variable, rate, event, tag });

/// Run order of one object's graph. Lower runs first. Zero is the ordinary order.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScriptOrder {
    pub order: i32,
}

text_component!(ScriptOrder as "script_order" { order });

fn num(value: f32) -> String {
    let text = format!("{value:.4}");
    let trimmed = text.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() || trimmed == "-" {
        "0".into()
    } else {
        trimmed.into()
    }
}

fn finite(text: &str) -> Option<f32> {
    text.trim().parse::<f32>().ok().filter(|n| n.is_finite())
}

/// The fields the inspector shows.
pub fn fields(volume: &Volume) -> Vec<(String, String)> {
    if volume.kind.is_empty() {
        return Vec::new();
    }
    let mut out = vec![("volume_kind".into(), volume.kind.clone())];
    match volume.kind.as_str() {
        "kill" => out.push(("volume_mode".into(), volume.mode.clone())),
        "damage" => {
            out.push(("volume_variable".into(), volume.variable.clone()));
            out.push(("volume_rate".into(), num(volume.rate)));
            out.push(("volume_event".into(), volume.event.clone()));
        }
        "checkpoint" => out.push(("volume_event".into(), volume.event.clone())),
        "trigger" => {
            out.push(("volume_tag".into(), volume.tag.clone()));
            out.push(("volume_event".into(), volume.event.clone()));
        }
        _ => {}
    }
    out
}

/// Why `value` is refused for `key`, or `None` when it is fine.
pub fn check(key: &str, value: &str) -> Option<&'static str> {
    let v = value.trim();
    let ok = match key {
        "volume_kind" => v.is_empty() || KINDS.contains(&v),
        "volume_mode" => KILL_MODES.contains(&v),
        "volume_variable" | "volume_event" | "volume_tag" => {
            !v.chars().any(|c| c == '\n' || c == '\r')
        }
        "volume_rate" => finite(v).is_some_and(|n| n >= 0.0),
        _ => return Some("unknown"),
    };
    (!ok).then_some("inspector-volume-invalid")
}

/// Set one field. Returns false, and changes nothing, when the text is refused.
pub fn apply(volume: &mut Volume, key: &str, value: &str) -> bool {
    if !KEYS.contains(&key) || check(key, value).is_some() {
        return false;
    }
    let v = value.trim();
    match key {
        "volume_kind" => volume.kind = if v == "none" { String::new() } else { v.into() },
        "volume_mode" => volume.mode = v.into(),
        "volume_variable" => volume.variable = v.into(),
        "volume_rate" => volume.rate = finite(v).expect("checked"),
        "volume_event" => volume.event = v.into(),
        "volume_tag" => volume.tag = v.into(),
        _ => return false,
    }
    true
}

pub fn field_text(volume: &Volume, key: &str) -> Option<String> {
    fields(volume).into_iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn outline(size: [f32; 2], color: [f32; 4]) -> seal_scene::Shape2d {
    let (w, h) = (size[0] * 0.5, size[1] * 0.5);
    seal_scene::Shape2d {
        kind: "line".into(),
        points: vec![[-w, -h], [w, -h], [w, h], [-w, h]],
        width: 0.06,
        color,
        closed: true,
        ..seal_scene::Shape2d::default()
    }
}

/// Fill in a freshly added object when `kind` is a volume preset.
pub fn configure(entity: &mut Entity, kind: &str) -> bool {
    let (volume, size, color) = match kind {
        "kill_zone" => (
            Volume { kind: "kill".into(), mode: "respawn".into(), ..Volume::default() },
            [4.0, 3.0],
            [0.85, 0.28, 0.28],
        ),
        "damage_zone" => (
            Volume {
                kind: "damage".into(),
                variable: "health".into(),
                rate: 10.0,
                event: String::new(),
                ..Volume::default()
            },
            [4.0, 3.0],
            [0.9, 0.5, 0.2],
        ),
        "checkpoint" => (
            Volume { kind: "checkpoint".into(), ..Volume::default() },
            [1.5, 2.5],
            [0.3, 0.75, 0.45],
        ),
        "trigger_volume" => {
            (Volume { kind: "trigger".into(), ..Volume::default() }, [3.0, 3.0], [0.85, 0.75, 0.25])
        }
        "post_process" => {
            (Volume { kind: "post".into(), ..Volume::default() }, [6.0, 4.0], [0.45, 0.55, 0.85])
        }
        _ => return false,
    };
    entity.color = color;
    entity.phys2d.body = "fixed".into();
    entity.phys2d.shape = "box".into();
    entity.phys2d.size = size;
    entity.volume = volume;
    if kind == "post_process" {
        entity.post = crate::post_volume::PostVolume::default();
    }
    entity.shape2d = outline(size, [color[0], color[1], color[2], 0.9]);
    true
}

/// Who is inside which volume, and leftover fractional damage.
#[derive(Clone, Debug, Default)]
pub struct Memory {
    inside: BTreeSet<(String, String)>,
    debt: BTreeMap<(String, String), f32>,
}

/// One event a volume produced this step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cue {
    pub event: String,
    pub volume: String,
    pub other: String,
}

/// What a step did, besides moving and scoring.
#[derive(Clone, Debug, Default)]
pub struct Outcome {
    pub cues: Vec<Cue>,
    /// Players a kill zone removed.
    pub remove: Vec<String>,
}

fn half(entity: &Entity) -> [f32; 2] {
    [
        entity.phys2d.size[0].abs() * entity.scale[0].abs() * 0.5,
        entity.phys2d.size[1].abs() * entity.scale[1].abs() * 0.5,
    ]
}

fn overlaps(a: &Entity, b: &Entity) -> bool {
    let (ha, hb) = (half(a), half(b));
    (a.translation[0] - b.translation[0]).abs() < ha[0] + hb[0]
        && (a.translation[1] - b.translation[1]).abs() < ha[1] + hb[1]
}

fn skip_other(entity: &Entity) -> bool {
    entity.hidden
        || entity.id == "root"
        || matches!(entity.tag.as_str(), "folder" | "camera" | "light")
        || !entity.volume.kind.is_empty()
}

fn is_player(entity: &Entity, game: &Game) -> bool {
    entity.tag == "player" || game.possessed.values().any(|id| id == &entity.id)
}

fn tag_ok(filter: &str, tag: &str) -> bool {
    let filter = filter.trim();
    if filter.is_empty() {
        return true;
    }
    filter.split(',').map(str::trim).filter(|s| !s.is_empty()).any(|s| s == tag)
}

fn hurt(game: &mut Game, name: &str, points: i64) {
    if points <= 0 {
        return;
    }
    let score = game.scores.entry(name.to_string()).or_insert(0);
    *score = score.saturating_sub(points);
    if let Some(text) = game.values.get_mut(name) {
        if let Ok(n) = text.trim().parse::<f64>() {
            let left = n - points as f64;
            *text = if left.fract() == 0.0 { format!("{left:.0}") } else { format!("{left}") };
        }
    }
}

/// Advance every volume by `dt` seconds. Players who enter a checkpoint are
/// remembered on `game`. Damage removes whole points from the named score, and
/// from a kept value of the same name when that value is a number.
pub fn step(entities: &mut [Entity], game: &mut Game, memory: &mut Memory, dt: f32) -> Outcome {
    let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
    let volumes: Vec<usize> = entities
        .iter()
        .enumerate()
        .filter(|(_, e)| !e.hidden && !e.volume.kind.is_empty())
        .map(|(i, _)| i)
        .collect();
    let mut now = BTreeSet::new();
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    for &vi in &volumes {
        for oi in 0..entities.len() {
            if oi == vi || skip_other(&entities[oi]) {
                continue;
            }
            if overlaps(&entities[vi], &entities[oi]) {
                now.insert((entities[vi].id.clone(), entities[oi].id.clone()));
                pairs.push((vi, oi));
            }
        }
    }
    let mut outcome = Outcome::default();
    for (vi, oi) in pairs {
        let (kind, mode, variable, rate, event, tag, at, vid, oid, player) = {
            let volume = &entities[vi];
            let other = &entities[oi];
            (
                volume.volume.kind.clone(),
                volume.volume.mode.clone(),
                if volume.volume.variable.trim().is_empty() {
                    "health".to_string()
                } else {
                    volume.volume.variable.trim().to_string()
                },
                volume.volume.rate,
                volume.volume.event.clone(),
                volume.volume.tag.clone(),
                volume.translation,
                volume.id.clone(),
                other.id.clone(),
                is_player(other, game),
            )
        };
        let key = (vid.clone(), oid.clone());
        let entered = !memory.inside.contains(&key);
        let cue = |name: String| Cue { event: name, volume: vid.clone(), other: oid.clone() };
        match kind.as_str() {
            "kill" if player && entered => {
                let name =
                    if event.trim().is_empty() { "killed".into() } else { event.trim().into() };
                outcome.cues.push(cue(name));
                let point = if mode == "destroy" {
                    None
                } else {
                    game.checkpoint.or_else(|| {
                        entities.iter().find(|e| e.tag == "player_start").map(|e| e.translation)
                    })
                };
                if let Some(point) = point {
                    if let Some(body) = entities.iter_mut().find(|e| e.id == oid) {
                        body.translation = point;
                        body.velocity = [0.0; 3];
                    }
                } else {
                    outcome.remove.push(oid.clone());
                }
            }
            "damage" if player => {
                let key_d = (vid.clone(), oid.clone());
                let acc = memory.debt.entry(key_d).or_insert(0.0);
                *acc += rate.max(0.0) * dt;
                let whole = (*acc + 1e-4).floor() as i64;
                if whole > 0 {
                    *acc -= whole as f32;
                    hurt(game, &variable, whole);
                    let name = if event.trim().is_empty() { "damaged".into() } else { event };
                    outcome.cues.push(cue(name));
                }
            }
            "checkpoint" if player && entered => {
                game.checkpoint = Some(at);
                let name = if event.trim().is_empty() { "checkpoint".into() } else { event };
                outcome.cues.push(cue(name));
            }
            "trigger" if tag_ok(&tag, &entities[oi].tag) => {
                let base = event.trim();
                let (enter_n, stay_n) = if base.is_empty() {
                    ("trigger_enter".to_string(), "trigger_stay".to_string())
                } else {
                    (format!("{base}_enter"), format!("{base}_stay"))
                };
                if entered {
                    outcome.cues.push(cue(enter_n));
                } else {
                    outcome.cues.push(cue(stay_n));
                }
            }
            _ => {}
        }
    }
    for (volume_id, other_id) in memory.inside.iter() {
        if now.contains(&(volume_id.clone(), other_id.clone())) {
            continue;
        }
        let Some(volume) = entities.iter().find(|e| &e.id == volume_id) else { continue };
        if volume.volume.kind != "trigger" {
            continue;
        }
        let Some(other) = entities.iter().find(|e| &e.id == other_id) else { continue };
        if !tag_ok(&volume.volume.tag, &other.tag) {
            continue;
        }
        let base = volume.volume.event.trim();
        let name =
            if base.is_empty() { "trigger_exit".to_string() } else { format!("{base}_exit") };
        outcome.cues.push(Cue { event: name, volume: volume_id.clone(), other: other_id.clone() });
    }
    memory.inside = now;
    memory.debt.retain(|key, _| memory.inside.contains(key));
    outcome
}
