//! Typed property tracks: one entity field, keyframes, and an easing into each key.
//!
//! Bool, text and enum values always hold until the next key. Numbers, vectors
//! and colours blend per component. Angles take the short way around the circle.

use seal_ecs::{SealMap, SealValue};

use crate::anim::{Interp, Track};
use crate::sequence::{SeqTrack, Sequence};

/// How the span into a key blends. Bool and text ignore this and step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ease {
    Step,
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
}

impl Ease {
    pub fn name(self) -> &'static str {
        match self {
            Ease::Step => "step",
            Ease::Linear => "linear",
            Ease::EaseIn => "ease-in",
            Ease::EaseOut => "ease-out",
            Ease::EaseInOut => "ease-in-out",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "step" => Ease::Step,
            "linear" => Ease::Linear,
            "ease-in" => Ease::EaseIn,
            "ease-out" => Ease::EaseOut,
            "ease-in-out" => Ease::EaseInOut,
            _ => return None,
        })
    }

    /// Unit curve at `u` in 0..1. Step stays on the previous key.
    pub fn curve(self, u: f32) -> f32 {
        let u = u.clamp(0.0, 1.0);
        match self {
            Ease::Step => 0.0,
            Ease::Linear => u,
            Ease::EaseIn => u * u,
            Ease::EaseOut => 1.0 - (1.0 - u) * (1.0 - u),
            Ease::EaseInOut => u * u * (3.0 - 2.0 * u),
        }
    }
}

/// One keyed value. The variant is the type; mixed types on one track step.
#[derive(Clone, Debug, PartialEq)]
pub enum PropValue {
    Number(f32),
    /// Degrees. Blends the short way around 360.
    Angle(f32),
    Bool(bool),
    /// Enum names and other text. Always steps.
    Text(String),
    /// Two, three or four components, blended independently.
    Vector(Vec<f32>),
    /// Degrees per component, each the short way.
    Angles(Vec<f32>),
    /// Channels from 0 to 1, blended independently. Three or four channels.
    Color(Vec<f32>),
}

impl PropValue {
    pub fn steps(&self) -> bool {
        matches!(self, PropValue::Bool(_) | PropValue::Text(_))
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            PropValue::Number(_) => "number",
            PropValue::Angle(_) => "angle",
            PropValue::Bool(_) => "bool",
            PropValue::Text(_) => "text",
            PropValue::Vector(_) => "vector",
            PropValue::Angles(_) => "angles",
            PropValue::Color(_) => "color",
        }
    }
}

/// A key. `ease` is the curve from the previous key into this one.
#[derive(Clone, Debug, PartialEq)]
pub struct PropKey {
    pub time: f32,
    pub value: PropValue,
    pub ease: Ease,
}

/// One field of one object, with typed keys.
#[derive(Clone, Debug, PartialEq)]
pub struct PropertyTrack {
    pub entity: String,
    pub path: String,
    pub keys: Vec<PropKey>,
}

impl PropertyTrack {
    /// Value at local time `t`. Empty tracks sample as nothing.
    /// Before the first key and after the last, the end key holds.
    pub fn sample(&self, t: f32) -> Option<PropValue> {
        let keys = &self.keys;
        let first = keys.first()?;
        if t <= first.time {
            return Some(first.value.clone());
        }
        let last = keys.last()?;
        if t >= last.time {
            return Some(last.value.clone());
        }
        let i = keys.iter().rposition(|key| key.time <= t)?;
        let left = &keys[i];
        let right = &keys[i + 1];
        let span = right.time - left.time;
        if span <= f32::EPSILON {
            return Some(right.value.clone());
        }
        let u = (t - left.time) / span;
        let ease = if left.value.steps() || right.value.steps() { Ease::Step } else { right.ease };
        Some(blend(&left.value, &right.value, ease.curve(u)))
    }
}

fn blend(left: &PropValue, right: &PropValue, u: f32) -> PropValue {
    if u <= 0.0 {
        return left.clone();
    }
    if u >= 1.0 {
        return right.clone();
    }
    match (left, right) {
        (PropValue::Number(a), PropValue::Number(b)) => PropValue::Number(a + (b - a) * u),
        (PropValue::Angle(a), PropValue::Angle(b)) => PropValue::Angle(lerp_angle(*a, *b, u)),
        (PropValue::Vector(a), PropValue::Vector(b)) if a.len() == b.len() => {
            PropValue::Vector(a.iter().zip(b).map(|(x, y)| x + (y - x) * u).collect())
        }
        (PropValue::Angles(a), PropValue::Angles(b)) if a.len() == b.len() => {
            PropValue::Angles(a.iter().zip(b).map(|(x, y)| lerp_angle(*x, *y, u)).collect())
        }
        (PropValue::Color(a), PropValue::Color(b)) if a.len() == b.len() => {
            PropValue::Color(a.iter().zip(b).map(|(x, y)| x + (y - x) * u).collect())
        }
        _ => left.clone(),
    }
}

/// Shortest signed turn from `from` to `to`, in degrees, then blend.
pub fn lerp_angle(from: f32, to: f32, u: f32) -> f32 {
    let mut delta = (to - from).rem_euclid(360.0);
    if delta > 180.0 {
        delta -= 360.0;
    }
    from + delta * u
}

/// True when `text` is the SealText sequence form (a `key = value` line).
pub fn is_sealtext(text: &str) -> bool {
    text.lines().any(|raw| {
        let line = raw.split('#').next().unwrap_or("").trim();
        let Some((name, _)) = line.split_once('=') else {
            return false;
        };
        !name.trim().is_empty() && !name.trim().contains(' ')
    })
}

pub fn parse_seal(text: &str) -> Result<Sequence, String> {
    let value = seal_ecs::parse_seal_text(text).map_err(|e| e.to_string())?;
    if value.as_map().is_none() {
        return Err("sequence file must be a map".into());
    }
    let length = match value.get("length") {
        None => 0.0,
        Some(v) => finite(v).map_err(|e| format!("length: {e}"))?.max(0.0),
    };
    let looping = match value.get("loop") {
        None => false,
        Some(SealValue::Bool(v)) => *v,
        Some(_) => return Err("loop must be true or false".into()),
    };
    let tracks = match value.get("tracks") {
        None => Vec::new(),
        Some(SealValue::Array(items)) => items
            .iter()
            .map(parse_old_track)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("tracks: {e}"))?,
        Some(_) => return Err("tracks must be a list".into()),
    };
    let properties = match value.get("properties") {
        None => Vec::new(),
        Some(SealValue::Array(items)) => items
            .iter()
            .map(parse_property)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("properties: {e}"))?,
        Some(_) => return Err("properties must be a list".into()),
    };
    let mut events = match value.get("events") {
        None => Vec::new(),
        Some(SealValue::Array(items)) => items
            .iter()
            .map(parse_event)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("events: {e}"))?,
        Some(_) => return Err("events must be a list".into()),
    };
    events.sort_by(|a, b| a.0.total_cmp(&b.0));
    let time_warp = match value.get("time_warp") {
        None => crate::time_warp::TimeWarp::default(),
        Some(SealValue::Array(items)) => {
            let mut keys = Vec::new();
            for item in items {
                let clock = finite_at(item, "clock").or_else(|_| finite_at(item, "time"))?;
                let content = finite_at(item, "content").or_else(|_| finite_at(item, "value"))?;
                if keys.last().is_some_and(|(t, _)| clock <= *t) {
                    return Err("time_warp keys must increase in clock time".into());
                }
                keys.push((clock, content));
            }
            crate::time_warp::TimeWarp { keys }
        }
        Some(_) => return Err("time_warp must be a list".into()),
    };
    Ok(Sequence { length, looping, tracks, properties, events, time_warp })
}

fn parse_old_track(value: &SealValue) -> Result<SeqTrack, String> {
    let object = text_at(value, "object")?;
    let target = text_at(value, "target")?;
    let easing = text_at(value, "easing").unwrap_or_else(|_| "linear".into());
    let interp = match easing.as_str() {
        "linear" => Interp::Linear,
        "step" => Interp::Step,
        "cubic" => Interp::Cubic,
        other => return Err(format!("unknown mode {other}")),
    };
    let mut keys = Vec::new();
    for key in list_at(value, "keys")? {
        let t = finite_at(&key, "time")?;
        let v = finite_at(&key, "value")?;
        if keys.last().is_some_and(|(prev, _)| t <= *prev) {
            return Err("keys must increase in time".into());
        }
        keys.push((t, v));
    }
    Ok(SeqTrack { object, track: Track { target, interp, keys } })
}

fn parse_property(value: &SealValue) -> Result<PropertyTrack, String> {
    let entity = text_at(value, "entity")?;
    let path = text_at(value, "path")?;
    if entity.is_empty() || path.is_empty() {
        return Err("a property track needs an entity and a path".into());
    }
    let mut keys = Vec::new();
    for key in list_at(value, "keys")? {
        let time = finite_at(&key, "time")?;
        if keys.last().is_some_and(|prev: &PropKey| time <= prev.time) {
            return Err("keys must increase in time".into());
        }
        let value = key
            .get("value")
            .ok_or_else(|| "a key needs a value".to_string())
            .and_then(parse_prop)?;
        let ease = match key.get("ease") {
            None => Ease::Linear,
            Some(SealValue::String(s)) => {
                Ease::parse(s).ok_or_else(|| format!("unknown easing {s}"))?
            }
            Some(_) => return Err("ease must be text".into()),
        };
        let ease = if value.steps() { Ease::Step } else { ease };
        keys.push(PropKey { time, value, ease });
    }
    Ok(PropertyTrack { entity, path, keys })
}

fn parse_event(value: &SealValue) -> Result<(f32, String), String> {
    Ok((finite_at(value, "time")?, text_at(value, "name")?))
}

fn parse_prop(value: &SealValue) -> Result<PropValue, String> {
    let kind = text_at(value, "type")?;
    let raw = value.get("value").ok_or_else(|| "property value needs a value".to_string())?;
    match kind.as_str() {
        "number" => Ok(PropValue::Number(finite(raw)?)),
        "angle" => Ok(PropValue::Angle(finite(raw)?)),
        "bool" => match raw {
            SealValue::Bool(v) => Ok(PropValue::Bool(*v)),
            _ => Err("bool value must be true or false".into()),
        },
        "text" => match raw {
            SealValue::String(v) => Ok(PropValue::Text(v.clone())),
            _ => Err("text value must be text".into()),
        },
        "vector" => Ok(PropValue::Vector(vec_at(raw, 2, 4)?)),
        "angles" => Ok(PropValue::Angles(vec_at(raw, 1, 3)?)),
        "color" => Ok(PropValue::Color(vec_at(raw, 3, 4)?)),
        other => Err(format!("unknown property type {other}")),
    }
}

fn vec_at(value: &SealValue, min: usize, max: usize) -> Result<Vec<f32>, String> {
    let SealValue::Array(items) = value else {
        return Err("expected a list of numbers".into());
    };
    if items.len() < min || items.len() > max {
        return Err(format!("expected {min} to {max} numbers"));
    }
    items.iter().map(finite).collect()
}

fn text_at(value: &SealValue, key: &str) -> Result<String, String> {
    match value.get(key) {
        Some(SealValue::String(v)) => Ok(v.clone()),
        _ => Err(format!("{key} must be text")),
    }
}

fn list_at(value: &SealValue, key: &str) -> Result<Vec<SealValue>, String> {
    match value.get(key) {
        None => Ok(Vec::new()),
        Some(SealValue::Array(items)) => Ok(items.clone()),
        Some(_) => Err(format!("{key} must be a list")),
    }
}

fn finite_at(value: &SealValue, key: &str) -> Result<f32, String> {
    match value.get(key) {
        Some(v) => finite(v).map_err(|e| format!("{key}: {e}")),
        None => Err(format!("missing {key}")),
    }
}

fn finite(value: &SealValue) -> Result<f32, String> {
    match value {
        SealValue::Number(text) => {
            let n: f32 = text.parse().map_err(|_| format!("{text} is not a number"))?;
            if n.is_finite() {
                Ok(n)
            } else {
                Err(format!("{text} is not a finite number"))
            }
        }
        _ => Err("expected a number".into()),
    }
}

pub fn write_seal(seq: &Sequence) -> String {
    let mut entries = vec![
        ("length".into(), number(seq.length)),
        ("loop".into(), SealValue::Bool(seq.looping)),
    ];
    if !seq.tracks.is_empty() {
        entries.push((
            "tracks".into(),
            SealValue::Array(seq.tracks.iter().map(write_old_track).collect()),
        ));
    }
    if !seq.properties.is_empty() {
        entries.push((
            "properties".into(),
            SealValue::Array(seq.properties.iter().map(write_property).collect()),
        ));
    }
    if !seq.events.is_empty() {
        entries.push((
            "events".into(),
            SealValue::Array(
                seq.events
                    .iter()
                    .map(|(time, name)| {
                        SealValue::Map(SealMap::new(vec![
                            ("time".into(), number(*time)),
                            ("name".into(), SealValue::String(name.clone())),
                        ]))
                    })
                    .collect(),
            ),
        ));
    }
    if !seq.time_warp.keys.is_empty() {
        entries.push((
            "time_warp".into(),
            SealValue::Array(
                seq
                    .time_warp
                    .keys
                    .iter()
                    .map(|(clock, content)| {
                        SealValue::Map(SealMap::new(vec![
                            ("clock".into(), number(*clock)),
                            ("content".into(), number(*content)),
                        ]))
                    })
                    .collect(),
            ),
        ));
    }
    SealValue::Map(SealMap::new(entries)).to_text()
}

fn write_old_track(track: &SeqTrack) -> SealValue {
    let easing = match track.track.interp {
        Interp::Linear => "linear",
        Interp::Step => "step",
        Interp::Cubic => "cubic",
    };
    SealValue::Map(SealMap::new(vec![
        ("object".into(), SealValue::String(track.object.clone())),
        ("target".into(), SealValue::String(track.track.target.clone())),
        ("easing".into(), SealValue::String(easing.into())),
        (
            "keys".into(),
            SealValue::Array(
                track
                    .track
                    .keys
                    .iter()
                    .map(|(time, value)| {
                        SealValue::Map(SealMap::new(vec![
                            ("time".into(), number(*time)),
                            ("value".into(), number(*value)),
                        ]))
                    })
                    .collect(),
            ),
        ),
    ]))
}

fn write_property(track: &PropertyTrack) -> SealValue {
    SealValue::Map(SealMap::new(vec![
        ("entity".into(), SealValue::String(track.entity.clone())),
        ("path".into(), SealValue::String(track.path.clone())),
        (
            "keys".into(),
            SealValue::Array(
                track
                    .keys
                    .iter()
                    .map(|key| {
                        SealValue::Map(SealMap::new(vec![
                            ("time".into(), number(key.time)),
                            ("ease".into(), SealValue::String(key.ease.name().into())),
                            ("value".into(), write_prop(&key.value)),
                        ]))
                    })
                    .collect(),
            ),
        ),
    ]))
}

fn write_prop(value: &PropValue) -> SealValue {
    let raw = match value {
        PropValue::Number(v) | PropValue::Angle(v) => number(*v),
        PropValue::Bool(v) => SealValue::Bool(*v),
        PropValue::Text(v) => SealValue::String(v.clone()),
        PropValue::Vector(v) | PropValue::Angles(v) | PropValue::Color(v) => {
            SealValue::Array(v.iter().copied().map(number).collect())
        }
    };
    SealValue::Map(SealMap::new(vec![
        ("type".into(), SealValue::String(value.type_name().into())),
        ("value".into(), raw),
    ]))
}

fn number(v: f32) -> SealValue {
    let v = if v.is_finite() { v } else { 0.0 };
    let text = format!("{v}");
    let text = if text.parse::<f32>() == Ok(v) {
        if text.contains('.') || text.contains('e') || text.contains('E') {
            text
        } else {
            format!("{text}.0")
        }
    } else {
        format!("{v:?}")
    };
    SealValue::Number(text)
}
