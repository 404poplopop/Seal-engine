//! Level 2 prediction: a stride predictor with a saturating confidence counter.
//!
//! A guess is offered only when the counter is at its maximum, and one wrong
//! guess resets it, so coverage is traded for accuracy. Predictions are never
//! committed unverified: [`FieldPredictors`] uses them to send or store only
//! the fields that were mispredicted, and the receiver runs the same
//! predictors to rebuild every value exactly.

/// One field's predictor over integer (or raw-bit) values.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stride {
    last: i64,
    stride: i64,
    confidence: u8,
    seen: u8,
}

impl Stride {
    /// The guess for the next value, when confidence is at `max`.
    pub fn predict(&self, max: u8) -> Option<i64> {
        (self.seen >= 2 && self.confidence >= max).then(|| self.last.wrapping_add(self.stride))
    }

    /// Learn the real value. Returns whether the raw guess (used or not) was right.
    pub fn update(&mut self, actual: i64, max: u8) -> bool {
        let right = self.seen >= 2 && self.last.wrapping_add(self.stride) == actual;
        if right {
            self.confidence = (self.confidence + 1).min(max);
        } else {
            self.confidence = 0;
        }
        if self.seen >= 1 {
            self.stride = actual.wrapping_sub(self.last);
        }
        self.last = actual;
        self.seen = (self.seen + 1).min(2);
        right
    }
}

/// Predictors for a set of fields, shared in lockstep by a sender and receiver.
#[derive(Clone, Debug)]
pub struct FieldPredictors {
    fields: Vec<Stride>,
    max: u8,
    pub offered: u64,
    pub used_right: u64,
    pub total: u64,
}

impl FieldPredictors {
    pub fn new(fields: usize, max_confidence: u8) -> Self {
        Self {
            fields: vec![Stride::default(); fields],
            max: max_confidence.max(1),
            offered: 0,
            used_right: 0,
            total: 0,
        }
    }

    /// Sender side: the fields whose prediction would be wrong (or missing),
    /// as `(index, value)`. Advances the predictors.
    pub fn encode(&mut self, values: &[i64]) -> Vec<(u32, i64)> {
        let mut out = Vec::new();
        for (i, (p, &v)) in self.fields.iter_mut().zip(values).enumerate() {
            self.total += 1;
            let guess = p.predict(self.max);
            if guess.is_some() {
                self.offered += 1;
            }
            if guess == Some(v) {
                self.used_right += 1;
            } else {
                out.push((i as u32, v));
            }
            p.update(v, self.max);
        }
        out
    }

    /// Receiver side: rebuild every value from the sent corrections.
    /// Returns `None` if a field had neither a prediction nor a correction.
    pub fn decode(&mut self, sent: &[(u32, i64)]) -> Option<Vec<i64>> {
        let mut out = Vec::with_capacity(self.fields.len());
        let mut next = sent.iter().peekable();
        for (i, p) in self.fields.iter_mut().enumerate() {
            let value = match next.peek() {
                Some(&&(j, v)) if j as usize == i => {
                    next.next();
                    v
                }
                _ => p.predict(self.max)?,
            };
            p.update(value, self.max);
            out.push(value);
        }
        Some(out)
    }

    /// Share of offered guesses that were right; 1 when none were offered.
    pub fn accuracy(&self) -> f64 {
        if self.offered == 0 {
            1.0
        } else {
            self.used_right as f64 / self.offered as f64
        }
    }

    pub fn coverage(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.offered as f64 / self.total as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lcg(seed: &mut u64) -> i64 {
        *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (*seed >> 33) as i64
    }

    fn run(kind: &str, ticks: i64) -> (f64, f64, usize) {
        let fields = 1000;
        let mut tx = FieldPredictors::new(fields, 3);
        let mut rx = FieldPredictors::new(fields, 3);
        let mut seed = 7u64;
        let mut sent = 0;
        for t in 0..ticks {
            let values: Vec<i64> = (0..fields as i64)
                .map(|i| match kind {
                    "counter" => i + t,
                    "constant" => i * 3,
                    "linear" => i + t * (i % 5 - 2),
                    // A period-8 cycle through scrambled values: needs a context predictor.
                    "periodic" => {
                        ((((t + i) % 8) as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 40) as i64
                    }
                    _ => lcg(&mut seed) % 1000,
                })
                .collect();
            let wire = tx.encode(&values);
            sent += wire.len();
            assert_eq!(rx.decode(&wire).unwrap(), values, "{kind} tick {t} rebuilds exactly");
        }
        (tx.coverage(), tx.accuracy(), sent)
    }

    #[test]
    fn predictable_fields_are_covered_and_never_wrong() {
        for kind in ["counter", "constant", "linear"] {
            let (coverage, accuracy, _) = run(kind, 200);
            assert!(coverage > 0.97, "{kind}: {coverage}");
            assert_eq!(accuracy, 1.0, "{kind}");
        }
    }

    #[test]
    fn unpredictable_fields_are_left_alone() {
        for kind in ["periodic", "random"] {
            let (coverage, _, sent) = run(kind, 200);
            assert!(coverage < 0.05, "{kind}: {coverage}");
            assert!(sent > 190 * 1000);
        }
    }

    #[test]
    fn one_wrong_guess_resets_confidence() {
        let mut s = Stride::default();
        for v in 0..6 {
            s.update(v, 3);
        }
        assert_eq!(s.predict(3), Some(6));
        assert!(!s.update(100, 3));
        assert_eq!(s.predict(3), None);
    }
}
