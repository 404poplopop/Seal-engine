//! Helpers for the editor profiler panel (history graph sizing).

/// Map step times (ms) to bar heights in 0..=1 for a small sparkline.
pub fn normalize_history(values: &[f32], max_ms: f32) -> Vec<f32> {
    let cap = if max_ms > 0.0 { max_ms } else { 1.0 };
    values.iter().map(|v| (v / cap).clamp(0.0, 1.0)).collect()
}