//! Sequence clock remapping (slow motion, reverse, hold).

#[derive(Clone, Debug, PartialEq, Default)]
pub struct TimeWarp {
    /// Keys sorted by clock time: maps sequence time to content time.
    pub keys: Vec<(f32, f32)>,
}

impl TimeWarp {
    pub fn sample(&self, t: f32) -> f32 {
        let keys = &self.keys;
        if keys.is_empty() {
            return t;
        }
        let first = keys[0];
        if t <= first.0 {
            return first.1;
        }
        let last = keys[keys.len() - 1];
        if t >= last.0 {
            return last.1;
        }
        let i = keys.iter().rposition(|k| k.0 <= t).unwrap_or(0);
        if i + 1 >= keys.len() {
            return keys[i].1;
        }
        let (t0, v0) = keys[i];
        let (t1, v1) = keys[i + 1];
        if (t1 - t0).abs() <= 1e-8 {
            return v1;
        }
        let u = (t - t0) / (t1 - t0);
        v0 + u * (v1 - v0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hold_and_reverse_segments() {
        let warp = TimeWarp {
            keys: vec![(0.0, 0.0), (1.0, 0.0), (2.0, -1.0), (3.0, 1.0)],
        };
        assert!((warp.sample(0.5) - 0.0).abs() < 1e-5);
        assert!(warp.sample(1.5) < 0.0);
    }
}