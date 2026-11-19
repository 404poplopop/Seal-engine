//! Timeline: keyframed tracks over several objects plus timed events (.seq text files).
//!
//! The original line form still loads:
//!
//!   length 4
//!   loop false
//!   object Door
//!   track position.x linear
//!   key 0 0
//!   key 2 3
//!   track visible step
//!   key 0 1
//!   event 1.5 door_open
//!
//! `object` starts a group; its tracks drive that object (position.*, rotation.*,
//! scale.*, color.*, visible, sprite_frame). Events carry no object.
//!
//! Files that assign `key = value` are SealText. Those round-trip through
//! [`Sequence::to_text`], including [`property`] tracks that drive any field.

pub use crate::property::{Ease, PropKey, PropValue, PropertyTrack};

use crate::anim::{Interp, Track};
use crate::time_warp::TimeWarp;

#[derive(Clone, Debug, PartialEq)]
pub struct SeqTrack {
    pub object: String,
    pub track: Track,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Sequence {
    pub length: f32,
    pub looping: bool,
    pub tracks: Vec<SeqTrack>,
    /// Typed tracks that drive any component field. Empty in a line-form file.
    pub properties: Vec<PropertyTrack>,
    pub events: Vec<(f32, String)>,
    /// Remaps sequence clock time to content time (slow motion, reverse, hold). Empty is linear.
    pub time_warp: TimeWarp,
}

impl Sequence {
    pub fn parse(text: &str) -> Result<Sequence, String> {
        if crate::property::is_sealtext(text) {
            return crate::property::parse_seal(text);
        }
        let mut s = Sequence {
            length: 0.0,
            looping: false,
            tracks: Vec::new(),
            properties: Vec::new(),
            events: Vec::new(),
            time_warp: TimeWarp::default(),
        };
        let mut object = String::new();
        for (n, raw) in text.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let w: Vec<&str> = line.split_whitespace().collect();
            let bad = |m: &str| format!("line {}: {}", n + 1, m);
            let num = |x: &str| x.parse::<f32>().map_err(|_| bad("not a number"));
            match w[0] {
                "length" if w.len() == 2 => s.length = num(w[1])?.max(0.0),
                "loop" if w.len() == 2 => s.looping = w[1] == "true",
                "object" if w.len() == 2 => object = w[1].into(),
                "track" if w.len() == 3 => {
                    if object.is_empty() {
                        return Err(bad("track before object"));
                    }
                    let interp = match w[2] {
                        "linear" => Interp::Linear,
                        "step" => Interp::Step,
                        "cubic" => Interp::Cubic,
                        _ => return Err(bad("unknown mode")),
                    };
                    s.tracks.push(SeqTrack {
                        object: object.clone(),
                        track: Track { target: w[1].into(), interp, keys: vec![] },
                    });
                }
                "key" if w.len() == 3 => {
                    let (t, v) = (num(w[1])?, num(w[2])?);
                    let tr = s.tracks.last_mut().ok_or_else(|| bad("key before track"))?;
                    if tr.track.keys.last().is_some_and(|k| t <= k.0) {
                        return Err(bad("keys must increase in time"));
                    }
                    tr.track.keys.push((t, v));
                }
                "event" if w.len() == 3 => s.events.push((num(w[1])?, w[2].into())),
                _ => return Err(bad("unknown line")),
            }
        }
        s.events.sort_by(|a, b| a.0.total_cmp(&b.0));
        Ok(s)
    }

    /// SealText form. A line-form file written back comes out in this form
    /// and parses to the same tracks, property tracks and events.
    pub fn to_text(&self) -> String {
        crate::property::write_seal(self)
    }

    pub fn local_time(&self, t: f32) -> f32 {
        if self.length <= 0.0 {
            0.0
        } else if self.looping {
            t.rem_euclid(self.length)
        } else {
            t.clamp(0.0, self.length)
        }
    }

    /// Map sequence clock time to content time using [`time_warp`].
    pub fn warp_time(&self, t: f32) -> f32 {
        let clock = self.local_time(t);
        self.time_warp.sample(clock)
    }

    /// (object, property, value) for every track at time `t`.
    pub fn sample(&self, t: f32) -> Vec<(&str, &str, f32)> {
        let t = self.warp_time(t);
        self.tracks
            .iter()
            .filter_map(|s| {
                s.track.sample(t).map(|v| (s.object.as_str(), s.track.target.as_str(), v))
            })
            .collect()
    }

    pub fn finished(&self, t: f32) -> bool {
        !self.looping && t >= self.length
    }

    /// Events passed when time moves from `from` (exclusive) to `to` (inclusive), in order.
    /// A looping sequence wraps; moving backwards (scrubbing) fires nothing.
    pub fn events_between(&self, from: f32, to: f32) -> Vec<&str> {
        if to <= from || self.length <= 0.0 {
            return vec![];
        }
        // Events sit on content time. Warp keeps the lap (looping) and times before the start,
        // so wrapping and the start event still fire; a reversed warp fires nothing, like scrubbing back.
        let (from, to) = if self.time_warp.keys.is_empty() {
            (from, to)
        } else {
            let warp = |t: f32| {
                if self.looping {
                    let lap = (t / self.length).floor();
                    lap * self.length + self.time_warp.sample(t - lap * self.length)
                } else if t < 0.0 {
                    t
                } else {
                    self.time_warp.sample(t.min(self.length))
                }
            };
            (warp(from), warp(to))
        };
        if to <= from {
            return vec![];
        }
        let mut out = vec![];
        if self.looping {
            let first = (from / self.length).floor();
            let last = (to / self.length).floor();
            let mut lap = first;
            while lap <= last {
                let (lo, hi) = (from - lap * self.length, to - lap * self.length);
                for (t, name) in &self.events {
                    if *t > lo && *t <= hi {
                        out.push(name.as_str());
                    }
                }
                lap += 1.0;
            }
        } else {
            let (from, to) = (from.max(-1.0), to.min(self.length));
            for (t, name) in &self.events {
                if *t > from && *t <= to {
                    out.push(name.as_str());
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod warp_tests {
    use super::*;

    #[test]
    fn warp_slow_motion_stretches_content() {
        let seq = Sequence {
            length: 4.0,
            looping: false,
            tracks: vec![SeqTrack {
                object: "a".into(),
                track: Track {
                    target: "position.x".into(),
                    interp: Interp::Linear,
                    keys: vec![(0.0, 0.0), (2.0, 10.0)],
                },
            }],
            properties: vec![],
            events: vec![],
            time_warp: TimeWarp { keys: vec![(0.0, 0.0), (4.0, 2.0)] },
        };
        let (_, _, v) = seq.sample(2.0).first().copied().unwrap();
        assert!((v - 5.0).abs() < 0.2);
    }
}
