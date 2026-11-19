//! Keyframe animation assets (.anim text files) and their sampling.
//!
//! Format, one item per line (`#` starts a comment):
//!   length 2.5
//!   loop true
//!   track position.x linear
//!   key 0 0
//!   key 1.5 4
//! Modes: linear, step, cubic (smoothstep ease between keys).
//! Tracks are named by the property they drive (position.x, color.r,
//! sprite_frame, ...); the caller applies sampled values.

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Interp {
    Linear,
    Step,
    Cubic,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    pub target: String,
    pub interp: Interp,
    pub keys: Vec<(f32, f32)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Anim {
    pub length: f32,
    pub looping: bool,
    pub tracks: Vec<Track>,
}

impl Track {
    /// Value at time `t`; before the first key and after the last it holds.
    pub fn sample(&self, t: f32) -> Option<f32> {
        let k = &self.keys;
        let (first, last) = (k.first()?, k.last()?);
        if t <= first.0 {
            return Some(first.1);
        }
        if t >= last.0 {
            return Some(last.1);
        }
        let i = k.iter().rposition(|key| key.0 <= t)?;
        let (a, b) = (k[i], k[i + 1]);
        let u = (t - a.0) / (b.0 - a.0);
        Some(match self.interp {
            Interp::Step => a.1,
            Interp::Linear => a.1 + (b.1 - a.1) * u,
            Interp::Cubic => a.1 + (b.1 - a.1) * (u * u * (3.0 - 2.0 * u)),
        })
    }
}

impl Anim {
    pub fn parse(text: &str) -> Result<Anim, String> {
        let mut a = Anim { length: 0.0, looping: false, tracks: vec![] };
        for (n, raw) in text.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let w: Vec<&str> = line.split_whitespace().collect();
            let bad = |m: &str| format!("line {}: {}", n + 1, m);
            let num = |s: &str| s.parse::<f32>().map_err(|_| bad("not a number"));
            match w[0] {
                "length" if w.len() == 2 => a.length = num(w[1])?.max(0.0),
                "loop" if w.len() == 2 => a.looping = w[1] == "true",
                "track" if w.len() == 3 => {
                    let interp = match w[2] {
                        "linear" => Interp::Linear,
                        "step" => Interp::Step,
                        "cubic" => Interp::Cubic,
                        _ => return Err(bad("unknown mode")),
                    };
                    a.tracks.push(Track { target: w[1].into(), interp, keys: vec![] });
                }
                "key" if w.len() == 3 => {
                    let (t, v) = (num(w[1])?, num(w[2])?);
                    let tr = a.tracks.last_mut().ok_or_else(|| bad("key before track"))?;
                    if tr.keys.last().is_some_and(|k| t <= k.0) {
                        return Err(bad("keys must increase in time"));
                    }
                    tr.keys.push((t, v));
                }
                _ => return Err(bad("unknown line")),
            }
        }
        Ok(a)
    }

    /// Wrap or clamp `t` to the animation's length.
    pub fn local_time(&self, t: f32) -> f32 {
        if self.length <= 0.0 {
            0.0
        } else if self.looping {
            t.rem_euclid(self.length)
        } else {
            t.clamp(0.0, self.length)
        }
    }

    /// Every track's value at time `t`.
    pub fn sample(&self, t: f32) -> Vec<(&str, f32)> {
        let t = self.local_time(t);
        self.tracks.iter().filter_map(|tr| tr.sample(t).map(|v| (tr.target.as_str(), v))).collect()
    }

    pub fn finished(&self, t: f32) -> bool {
        !self.looping && t >= self.length
    }
}
