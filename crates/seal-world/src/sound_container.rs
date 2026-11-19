//! Sound containers: a `.soundset` file groups several sounds and picks
//! which one to play using random, shuffle, sequence or layered rules.

use std::fmt;

/// File extension of sound container assets.
pub const EXTENSION: &str = "soundset";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContainerMode {
    Random,
    Shuffle,
    Sequence,
    Layered,
}

impl ContainerMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "random" => Some(Self::Random),
            "shuffle" => Some(Self::Shuffle),
            "sequence" | "sequential" => Some(Self::Sequence),
            "layered" | "layer" => Some(Self::Layered),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Random => "random",
            Self::Shuffle => "shuffle",
            Self::Sequence => "sequence",
            Self::Layered => "layered",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SoundEntry {
    pub path: String,
    pub weight: f32,
    pub volume: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SoundContainerDef {
    pub mode: ContainerMode,
    pub seed: u32,
    pub volume: f32,
    pub pitch_min: f32,
    pub pitch_max: f32,
    pub volume_var_min: f32,
    pub volume_var_max: f32,
    pub max_voices: u32,
    pub cooldown: f32,
    pub entries: Vec<SoundEntry>,
}

impl Default for SoundContainerDef {
    fn default() -> Self {
        Self {
            mode: ContainerMode::Random,
            seed: 1,
            volume: 1.0,
            pitch_min: 1.0,
            pitch_max: 1.0,
            volume_var_min: 1.0,
            volume_var_max: 1.0,
            max_voices: 8,
            cooldown: 0.0,
            entries: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlayPick {
    pub path: String,
    pub volume: f32,
    pub pitch: f32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContainerError {
    Parse(String),
    Empty,
    Busy,
    Cooldown,
}

impl fmt::Display for ContainerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(s) => write!(f, "{s}"),
            Self::Empty => write!(f, "sound container has no entries"),
            Self::Busy => write!(f, "too many voices are playing"),
            Self::Cooldown => write!(f, "sound container is on cooldown"),
        }
    }
}

impl std::error::Error for ContainerError {}

impl SoundContainerDef {
    pub fn is_soundset_path(path: &str) -> bool {
        path.trim_end_matches(['/', '\\'])
            .rsplit_once('.')
            .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case(EXTENSION))
    }

    /// Parse a `.soundset` text file.
    pub fn parse(text: &str) -> Result<Self, ContainerError> {
        let mut d = SoundContainerDef::default();
        for (number, raw) in text.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let err = |what: &str| ContainerError::Parse(format!("line {}: {what}", number + 1));
            let mut parts = line.split_whitespace();
            let key = parts.next().unwrap_or("");
            match key {
                "mode" => {
                    let v = parts.next().ok_or_else(|| err("mode needs a value"))?;
                    d.mode = ContainerMode::parse(v).ok_or_else(|| err("unknown mode"))?;
                }
                "seed" => {
                    let v: f32 = parts
                        .next()
                        .ok_or_else(|| err("seed needs a number"))?
                        .parse()
                        .map_err(|_| err("seed is not a number"))?;
                    d.seed = v.max(1.0) as u32;
                }
                "volume" => {
                    d.volume = one_f32(&mut parts, "volume")?;
                }
                "pitch" => {
                    let (a, b) = two_f32(&mut parts, "pitch")?;
                    d.pitch_min = a.min(b);
                    d.pitch_max = a.max(b);
                }
                "volume_var" => {
                    let (a, b) = two_f32(&mut parts, "volume_var")?;
                    d.volume_var_min = a.min(b).max(0.0);
                    d.volume_var_max = a.max(b).max(0.0);
                }
                "max_voices" => {
                    let v: f32 = parts
                        .next()
                        .ok_or_else(|| err("max_voices needs a number"))?
                        .parse()
                        .map_err(|_| err("max_voices is not a number"))?;
                    d.max_voices = v.max(0.0) as u32;
                }
                "cooldown" => {
                    d.cooldown = one_f32(&mut parts, "cooldown")?.max(0.0);
                }
                "entry" => {
                    let rest = line.strip_prefix("entry").ok_or_else(|| err("entry line"))?.trim();
                    let tokens: Vec<&str> = rest.split_whitespace().collect();
                    let cut = tokens
                        .iter()
                        .position(|t| matches!(*t, "weight" | "volume"))
                        .unwrap_or(tokens.len());
                    if cut == 0 {
                        return Err(err("entry needs a sound path"));
                    }
                    let path = tokens[..cut].join(" ");
                    let mut weight = 1.0f32;
                    let mut volume = 1.0f32;
                    let mut i = cut;
                    while i < tokens.len() {
                        match tokens[i] {
                            "weight" => {
                                i += 1;
                                weight = tokens
                                    .get(i)
                                    .ok_or_else(|| err("weight needs a number"))?
                                    .parse::<f32>()
                                    .map_err(|_| err("weight is not a number"))?
                                    .max(0.0);
                            }
                            "volume" => {
                                i += 1;
                                volume = tokens
                                    .get(i)
                                    .ok_or_else(|| err("volume needs a number"))?
                                    .parse::<f32>()
                                    .map_err(|_| err("volume is not a number"))?
                                    .max(0.0);
                            }
                            other => return Err(err(&format!("unknown entry field {other}"))),
                        }
                        i += 1;
                    }
                    d.entries.push(SoundEntry { path, weight, volume });
                }
                other => return Err(err(&format!("unknown key `{other}`"))),
            }
        }
        Ok(d)
    }

    pub fn to_text(&self) -> String {
        let mut s = String::from("# Seal sound container\n");
        s.push_str(&format!("mode {}\n", self.mode.name()));
        s.push_str(&format!("seed {}\n", self.seed));
        s.push_str(&format!("volume {}\n", num(self.volume)));
        if self.pitch_min != 1.0 || self.pitch_max != 1.0 {
            s.push_str(&format!("pitch {} {}\n", num(self.pitch_min), num(self.pitch_max)));
        }
        if self.volume_var_min != 1.0 || self.volume_var_max != 1.0 {
            s.push_str(&format!(
                "volume_var {} {}\n",
                num(self.volume_var_min),
                num(self.volume_var_max)
            ));
        }
        if self.max_voices != 8 {
            s.push_str(&format!("max_voices {}\n", self.max_voices));
        }
        if self.cooldown > 0.0 {
            s.push_str(&format!("cooldown {}\n", num(self.cooldown)));
        }
        for e in &self.entries {
            let mut line = format!("entry {}", e.path);
            if e.weight != 1.0 {
                line.push_str(&format!(" weight {}", num(e.weight)));
            }
            if e.volume != 1.0 {
                line.push_str(&format!(" volume {}", num(e.volume)));
            }
            line.push('\n');
            s.push_str(&line);
        }
        s
    }
}

fn num(v: f32) -> String {
    if v.fract() == 0.0 && v.abs() < 1e9 {
        format!("{}", v as i64)
    } else {
        format!("{v:.6}").trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

fn one_f32<'a, I: Iterator<Item = &'a str>>(
    parts: &mut I,
    field: &str,
) -> Result<f32, ContainerError> {
    parts
        .next()
        .ok_or_else(|| ContainerError::Parse(format!("{field} needs a number")))?
        .parse::<f32>()
        .map_err(|_| ContainerError::Parse(format!("{field} is not a number")))
}

fn two_f32<'a, I: Iterator<Item = &'a str>>(
    parts: &mut I,
    field: &str,
) -> Result<(f32, f32), ContainerError> {
    Ok((one_f32(parts, field)?, one_f32(parts, field)?))
}

#[derive(Clone, Debug)]
pub struct SoundContainerRuntime {
    def: SoundContainerDef,
    rng: u32,
    last_index: Option<usize>,
    sequence_index: usize,
    shuffle_bag: Vec<usize>,
    cooldown_left: f32,
    active_voices: u32,
    time: f32,
    voice_release: Vec<f32>,
}

impl SoundContainerRuntime {
    pub fn new(def: SoundContainerDef) -> Self {
        let rng = def.seed.max(1);
        Self {
            def,
            rng,
            last_index: None,
            sequence_index: 0,
            shuffle_bag: Vec::new(),
            cooldown_left: 0.0,
            active_voices: 0,
            time: 0.0,
            voice_release: Vec::new(),
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new(self.def.clone());
    }

    pub fn tick(&mut self, dt: f32) {
        let dt = dt.max(0.0);
        self.time += dt;
        if self.cooldown_left > 0.0 {
            self.cooldown_left = (self.cooldown_left - dt).max(0.0);
        }
        let now = self.time;
        self.voice_release.retain(|&t| {
            if t <= now {
                self.active_voices = self.active_voices.saturating_sub(1);
                false
            } else {
                true
            }
        });
    }

    /// Release one voice slot (call when a sound finishes).
    pub fn voice_done(&mut self) {
        self.active_voices = self.active_voices.saturating_sub(1);
    }

    fn rand_u32(&mut self) -> u32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        self.rng
    }

    fn rand_unit(&mut self) -> f32 {
        (self.rand_u32() >> 8) as f32 / (1u32 << 24) as f32
    }

    fn rand_f32(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.rand_unit()
    }

    fn pick_indices(&mut self) -> Result<Vec<usize>, ContainerError> {
        let n = self.def.entries.len();
        if n == 0 {
            return Err(ContainerError::Empty);
        }
        match self.def.mode {
            ContainerMode::Layered => Ok((0..n).collect()),
            ContainerMode::Sequence => {
                let i = self.sequence_index % n;
                self.sequence_index = self.sequence_index.wrapping_add(1);
                Ok(vec![i])
            }
            ContainerMode::Shuffle => {
                if self.shuffle_bag.is_empty() {
                    self.shuffle_bag = (0..n).collect();
                    // Fisher–Yates shuffle.
                    for i in (1..n).rev() {
                        let j = (self.rand_u32() as usize) % (i + 1);
                        self.shuffle_bag.swap(i, j);
                    }
                }
                let i = self.shuffle_bag.pop().unwrap();
                Ok(vec![i])
            }
            ContainerMode::Random => {
                if n == 1 {
                    return Ok(vec![0]);
                }
                let total: f32 = self.def.entries.iter().map(|e| e.weight.max(0.0)).sum();
                if total <= 0.0 {
                    return Ok(vec![0]);
                }
                let mut tries = 0;
                loop {
                    tries += 1;
                    let mut roll = self.rand_unit() * total;
                    let mut pick = 0;
                    for (i, e) in self.def.entries.iter().enumerate() {
                        roll -= e.weight.max(0.0);
                        if roll <= 0.0 {
                            pick = i;
                            break;
                        }
                    }
                    if self.last_index != Some(pick) || tries > n * 4 {
                        self.last_index = Some(pick);
                        return Ok(vec![pick]);
                    }
                }
            }
        }
    }

    fn make_pick(&mut self, index: usize, base_volume: f32, base_pitch: f32) -> PlayPick {
        let (path, entry_vol) = {
            let e = &self.def.entries[index];
            (e.path.clone(), e.volume)
        };
        let vol_scale = self.rand_f32(self.def.volume_var_min, self.def.volume_var_max);
        let pitch = if base_pitch != 1.0 {
            base_pitch
        } else {
            self.rand_f32(self.def.pitch_min, self.def.pitch_max)
        };
        PlayPick {
            path,
            volume: (base_volume * self.def.volume * entry_vol * vol_scale).max(0.0),
            pitch: pitch.max(0.01),
        }
    }

    /// Attempt to play the container. Pass `base_volume` and `base_pitch` from the caller
    /// (for example a script node). Returns an error when empty, on cooldown, or at voice cap.
    pub fn play(
        &mut self,
        base_volume: f32,
        base_pitch: f32,
    ) -> Result<Vec<PlayPick>, ContainerError> {
        if self.def.entries.is_empty() {
            return Err(ContainerError::Empty);
        }
        if self.cooldown_left > 0.0 {
            return Err(ContainerError::Cooldown);
        }
        let count = match self.def.mode {
            ContainerMode::Layered => self.def.entries.len(),
            _ => 1,
        };
        let max = self.def.max_voices.max(1);
        if self.active_voices + count as u32 > max {
            return Err(ContainerError::Busy);
        }
        let indices = self.pick_indices()?;
        let picks: Vec<PlayPick> =
            indices.into_iter().map(|i| self.make_pick(i, base_volume, base_pitch)).collect();
        self.active_voices += count as u32;
        let hold = 2.0f32;
        for _ in 0..count {
            self.voice_release.push(self.time + hold);
        }
        if self.def.cooldown > 0.0 {
            self.cooldown_left = self.def.cooldown;
        }
        Ok(picks)
    }

    /// Like [`play`](Self::play) but ignores cooldown and voice limits (for editor preview).
    pub fn preview(
        &mut self,
        base_volume: f32,
        base_pitch: f32,
    ) -> Result<Vec<PlayPick>, ContainerError> {
        if self.def.entries.is_empty() {
            return Err(ContainerError::Empty);
        }
        let indices = self.pick_indices()?;
        Ok(indices.into_iter().map(|i| self.make_pick(i, base_volume, base_pitch)).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
# Seal sound container
mode sequence
seed 42
volume 0.8
pitch 0.9 1.1
volume_var 0.5 1.0
max_voices 2
cooldown 0.5
entry a.wav weight 1 volume 1
entry b.wav weight 2 volume 0.5
entry c.wav weight 1 volume 1
";

    #[test]
    fn text_round_trip() {
        let def = SoundContainerDef::parse(SAMPLE).unwrap();
        let again = SoundContainerDef::parse(&def.to_text()).unwrap();
        assert_eq!(def, again);
    }

    #[test]
    fn damaged_file_is_an_error() {
        assert!(SoundContainerDef::parse("mode nope\n").is_err());
        assert!(SoundContainerDef::parse("entry\n").is_err());
        assert!(SoundContainerDef::parse("volume x\n").is_err());
    }

    fn runtime(mode: ContainerMode, entries: &[(&str, f32)]) -> SoundContainerRuntime {
        let def = SoundContainerDef {
            mode,
            seed: 7,
            cooldown: 0.0,
            max_voices: 99,
            entries: entries
                .iter()
                .map(|(p, w)| SoundEntry { path: (*p).into(), weight: *w, volume: 1.0 })
                .collect(),
            ..SoundContainerDef::default()
        };
        SoundContainerRuntime::new(def)
    }

    #[test]
    fn sequence_order_is_fixed() {
        let mut r = runtime(ContainerMode::Sequence, &[("a", 1.0), ("b", 1.0), ("c", 1.0)]);
        let paths: Vec<_> =
            (0..5).map(|_| r.play(1.0, 1.0).unwrap().into_iter().next().unwrap().path).collect();
        assert_eq!(paths, ["a", "b", "c", "a", "b"]);
    }

    #[test]
    fn shuffle_uses_every_entry_once_per_cycle() {
        let mut r = runtime(ContainerMode::Shuffle, &[("a", 1.0), ("b", 1.0), ("c", 1.0)]);
        let mut bag = Vec::new();
        for _ in 0..6 {
            bag.push(r.play(1.0, 1.0).unwrap()[0].path.clone());
        }
        assert_eq!(bag[0..3].iter().collect::<std::collections::BTreeSet<_>>().len(), 3);
        assert_eq!(bag[3..6].iter().collect::<std::collections::BTreeSet<_>>().len(), 3);
    }

    #[test]
    fn random_skips_immediate_repeat() {
        let mut r = runtime(ContainerMode::Random, &[("a", 1.0), ("b", 1.0), ("c", 1.0)]);
        let mut last = String::new();
        for _ in 0..20 {
            let p = r.play(1.0, 1.0).unwrap()[0].path.clone();
            assert_ne!(p, last);
            last = p;
            r.voice_done();
        }
    }

    #[test]
    fn weights_skew_picks() {
        let mut r = runtime(ContainerMode::Random, &[("a", 1.0), ("b", 50.0), ("c", 1.0)]);
        let mut b = 0;
        for _ in 0..300 {
            if r.play(1.0, 1.0).unwrap()[0].path == "b" {
                b += 1;
            }
            r.voice_done();
        }
        assert!(b > 200, "expected heavy weight to dominate, got {b}");
    }

    #[test]
    fn layered_plays_all_at_once() {
        let mut r = runtime(ContainerMode::Layered, &[("a", 1.0), ("b", 1.0)]);
        let picks = r.play(1.0, 1.0).unwrap();
        assert_eq!(picks.len(), 2);
        assert_eq!(picks[0].path, "a");
        assert_eq!(picks[1].path, "b");
    }

    #[test]
    fn cooldown_and_max_voices() {
        let def = SoundContainerDef {
            mode: ContainerMode::Sequence,
            seed: 1,
            cooldown: 1.0,
            max_voices: 1,
            entries: vec![SoundEntry { path: "a".into(), weight: 1.0, volume: 1.0 }],
            ..Default::default()
        };
        let mut r = SoundContainerRuntime::new(def);
        assert!(r.play(1.0, 1.0).is_ok());
        assert!(matches!(r.play(1.0, 1.0), Err(ContainerError::Cooldown)));
        r.tick(2.0);
        assert!(r.play(1.0, 1.0).is_ok());
        r.voice_done();
        let busy = SoundContainerDef {
            mode: ContainerMode::Sequence,
            seed: 1,
            cooldown: 0.0,
            max_voices: 1,
            entries: vec![SoundEntry { path: "a".into(), weight: 1.0, volume: 1.0 }],
            ..Default::default()
        };
        let mut r2 = SoundContainerRuntime::new(busy);
        assert!(r2.play(1.0, 1.0).is_ok());
        assert!(matches!(r2.play(1.0, 1.0), Err(ContainerError::Busy)));
    }
}
