//! Damaged-input checks: loaders must turn broken files and packets into errors, never into a crash.

use std::panic::{catch_unwind, AssertUnwindSafe};

pub struct Rng(u64);
impl Rng {
    pub fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n.max(1) as u64) as usize
    }
}

const PIECES: &[&str] = &[
    " ",
    "\n",
    "-",
    "1e309",
    "NaN",
    "inf",
    "-0",
    "999999999999999999999",
    "\"",
    "{",
    "}",
    "[",
    "]",
    ":",
    "=",
    "key",
    "length",
    "event",
    "track",
    "object",
    "rate",
    "max",
    "size",
    "color",
    "é",
    "\u{0}",
    "\t",
    "0",
    "-1",
];

/// Truncations, byte flips, deleted spans and inserted tokens of a valid sample.
pub fn mutants(sample: &[u8], seed: u64, count: usize) -> Vec<Vec<u8>> {
    let mut rng = Rng(seed | 1);
    let mut out = vec![Vec::new()];
    for cut in 0..sample.len().min(64) {
        out.push(sample[..cut].to_vec());
    }
    for _ in 0..count {
        let mut bytes = sample.to_vec();
        for _ in 0..1 + rng.below(4) {
            match rng.below(4) {
                0 if !bytes.is_empty() => {
                    let at = rng.below(bytes.len());
                    bytes[at] = rng.next_u64() as u8;
                }
                1 if !bytes.is_empty() => {
                    let at = rng.below(bytes.len());
                    let end = (at + rng.below(16)).min(bytes.len());
                    bytes.drain(at..end);
                }
                2 => {
                    let at = rng.below(bytes.len() + 1);
                    let piece = PIECES[rng.below(PIECES.len())].as_bytes();
                    bytes.splice(at..at, piece.iter().copied());
                }
                _ => bytes.truncate(rng.below(bytes.len() + 1)),
            }
        }
        out.push(bytes);
    }
    out
}

pub fn survives(name: &str, sample: &[u8], load: impl Fn(&[u8])) {
    for (i, input) in mutants(sample, 0x5ea1 + name.len() as u64, 400).into_iter().enumerate() {
        let result = catch_unwind(AssertUnwindSafe(|| load(&input)));
        assert!(
            result.is_ok(),
            "{name} crashed on mutant {i}: {:?}",
            String::from_utf8_lossy(&input)
        );
    }
}

pub fn text(load: impl Fn(&str)) -> impl Fn(&[u8]) {
    move |bytes| load(&String::from_utf8_lossy(bytes))
}
