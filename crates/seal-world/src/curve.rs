//! Curves and gradients sampled with plain arithmetic.
//!
//! Keys do not have to be sorted. Sampling sorts a copy by time.

/// How the segment that leaves a key reaches the next key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CurveEase {
    /// Cubic blend using the key tangents (value change per unit of time).
    Tangent,
    /// Hold this key's value until the next key.
    Constant,
    /// Straight line.
    Linear,
    /// Slow at the start of the segment.
    EaseIn,
    /// Slow at the end of the segment.
    EaseOut,
    /// Slow at both ends of the segment.
    EaseInOut,
}

/// One point on a curve.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CurveKey {
    pub time: f32,
    pub value: f32,
    /// Slope arriving at this key, used when the previous key's easing is [`CurveEase::Tangent`].
    pub in_tangent: f32,
    /// Slope leaving this key, used when this key's easing is [`CurveEase::Tangent`].
    pub out_tangent: f32,
    pub ease: CurveEase,
}

/// A list of keys. Empty samples as 0.
#[derive(Clone, Debug, PartialEq)]
pub struct Curve {
    pub keys: Vec<CurveKey>,
}

/// Whole-curve shapes offered in the editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CurvePreset {
    Constant,
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
}

/// A colour at a time, red green and blue, each usually 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorStop {
    pub time: f32,
    pub rgb: [f32; 3],
}

/// Opacity at a time, usually 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AlphaStop {
    pub time: f32,
    pub alpha: f32,
}

/// Colour stops and alpha stops, blended separately over time.
#[derive(Clone, Debug, PartialEq)]
pub struct Gradient {
    pub colors: Vec<ColorStop>,
    pub alphas: Vec<AlphaStop>,
}

impl CurveKey {
    pub fn new(time: f32, value: f32, ease: CurveEase) -> Self {
        Self { time, value, in_tangent: 0.0, out_tangent: 0.0, ease }
    }
}

impl Default for Curve {
    fn default() -> Self {
        Self::fallback()
    }
}

impl Curve {
    /// Straight line from 0 at time 0 to 1 at time 1. Damaged text falls back to this.
    pub fn fallback() -> Self {
        Self::linear(0.0, 1.0)
    }

    /// Two keys, straight from `start` at time 0 to `end` at time 1.
    pub fn linear(start: f32, end: f32) -> Self {
        Self {
            keys: vec![
                CurveKey::new(0.0, start, CurveEase::Linear),
                CurveKey::new(1.0, end, CurveEase::Linear),
            ],
        }
    }

    /// One key. Sampling returns `value` at every time.
    pub fn constant(value: f32) -> Self {
        Self { keys: vec![CurveKey::new(0.0, value, CurveEase::Constant)] }
    }
}

impl Default for Gradient {
    fn default() -> Self {
        Self::fallback()
    }
}

impl Gradient {
    /// Opaque white. Damaged text falls back to this.
    pub fn fallback() -> Self {
        Self {
            colors: vec![ColorStop { time: 0.0, rgb: [1.0, 1.0, 1.0] }],
            alphas: vec![AlphaStop { time: 0.0, alpha: 1.0 }],
        }
    }

    /// Straight blend from `start` at time 0 to `end` at time 1.
    pub fn from_endpoints(start: [f32; 4], end: [f32; 4]) -> Self {
        Self {
            colors: vec![
                ColorStop { time: 0.0, rgb: [start[0], start[1], start[2]] },
                ColorStop { time: 1.0, rgb: [end[0], end[1], end[2]] },
            ],
            alphas: vec![
                AlphaStop { time: 0.0, alpha: start[3] },
                AlphaStop { time: 1.0, alpha: end[3] },
            ],
        }
    }
}

fn finite(value: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        0.0
    }
}

fn ordered_keys(keys: &[CurveKey]) -> Vec<CurveKey> {
    let mut copy: Vec<CurveKey> = keys
        .iter()
        .copied()
        .map(|mut key| {
            key.time = finite(key.time);
            key.value = finite(key.value);
            key.in_tangent = finite(key.in_tangent);
            key.out_tangent = finite(key.out_tangent);
            key
        })
        .collect();
    copy.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap_or(std::cmp::Ordering::Equal));
    copy
}

/// Value of `curve` at `t`. Before the first key and after the last key the end value is held.
/// One key returns that value at every time. Keys are sorted by time first.
pub fn evaluate(curve: &Curve, t: f32) -> f32 {
    let keys = ordered_keys(&curve.keys);
    let Some(first) = keys.first() else {
        return 0.0;
    };
    if keys.len() == 1 || !t.is_finite() {
        return first.value;
    }
    let last = keys[keys.len() - 1];
    if t <= first.time {
        return first.value;
    }
    if t >= last.time {
        return last.value;
    }
    let right = keys.iter().position(|key| key.time >= t).unwrap_or(keys.len() - 1);
    if right == 0 {
        return first.value;
    }
    let left = &keys[right - 1];
    let right = &keys[right];
    let span = right.time - left.time;
    if span <= 1.0e-8 {
        return right.value;
    }
    let u = ((t - left.time) / span).clamp(0.0, 1.0);
    match left.ease {
        CurveEase::Constant => left.value,
        CurveEase::Linear => left.value + (right.value - left.value) * u,
        CurveEase::EaseIn => {
            let s = u * u;
            left.value + (right.value - left.value) * s
        }
        CurveEase::EaseOut => {
            let v = 1.0 - u;
            let s = 1.0 - v * v;
            left.value + (right.value - left.value) * s
        }
        CurveEase::EaseInOut => {
            let s = u * u * (3.0 - 2.0 * u);
            left.value + (right.value - left.value) * s
        }
        CurveEase::Tangent => {
            let u2 = u * u;
            let u3 = u2 * u;
            let h00 = 2.0 * u3 - 3.0 * u2 + 1.0;
            let h10 = u3 - 2.0 * u2 + u;
            let h01 = -2.0 * u3 + 3.0 * u2;
            let h11 = u3 - u2;
            h00 * left.value
                + h10 * left.out_tangent * span
                + h01 * right.value
                + h11 * right.in_tangent * span
        }
    }
}

/// Replace the keys with a two-key shape. Endpoint times stay. Constant keeps the first value.
pub fn apply_preset(curve: &mut Curve, preset: CurvePreset) {
    let keys = ordered_keys(&curve.keys);
    let (t0, v0, t1, v1) = if keys.len() >= 2 {
        (keys[0].time, keys[0].value, keys[keys.len() - 1].time, keys[keys.len() - 1].value)
    } else if let Some(key) = keys.first() {
        (key.time, key.value, key.time + 1.0, key.value)
    } else {
        (0.0, 0.0, 1.0, 1.0)
    };
    let ease = match preset {
        CurvePreset::Constant => CurveEase::Constant,
        CurvePreset::Linear => CurveEase::Linear,
        CurvePreset::EaseIn => CurveEase::EaseIn,
        CurvePreset::EaseOut => CurveEase::EaseOut,
        CurvePreset::EaseInOut => CurveEase::EaseInOut,
    };
    let end = if preset == CurvePreset::Constant { v0 } else { v1 };
    curve.keys = vec![
        CurveKey::new(t0, v0, ease),
        CurveKey::new(if t1 > t0 { t1 } else { t0 + 1.0 }, end, ease),
    ];
}

/// Append a key and return its index.
pub fn add_key(curve: &mut Curve, time: f32, value: f32) -> usize {
    curve.keys.push(CurveKey::new(finite(time), finite(value), CurveEase::Linear));
    curve.keys.len() - 1
}

/// Move one key. Returns false when `index` is out of range.
pub fn move_key(curve: &mut Curve, index: usize, time: f32, value: f32) -> bool {
    let Some(key) = curve.keys.get_mut(index) else {
        return false;
    };
    key.time = finite(time);
    key.value = finite(value);
    true
}

/// Remove one key. Returns false when `index` is out of range.
pub fn delete_key(curve: &mut Curve, index: usize) -> bool {
    if index >= curve.keys.len() {
        return false;
    }
    curve.keys.remove(index);
    true
}

/// Replace the numbers and easing of one key.
pub fn set_key(
    curve: &mut Curve,
    index: usize,
    time: f32,
    value: f32,
    in_tangent: f32,
    out_tangent: f32,
    ease: CurveEase,
) -> bool {
    let Some(key) = curve.keys.get_mut(index) else {
        return false;
    };
    key.time = finite(time);
    key.value = finite(value);
    key.in_tangent = finite(in_tangent);
    key.out_tangent = finite(out_tangent);
    key.ease = ease;
    true
}

fn ordered_colors(stops: &[ColorStop]) -> Vec<ColorStop> {
    let mut copy: Vec<ColorStop> = stops
        .iter()
        .copied()
        .map(|mut stop| {
            stop.time = finite(stop.time);
            stop.rgb = [finite(stop.rgb[0]), finite(stop.rgb[1]), finite(stop.rgb[2])];
            stop
        })
        .collect();
    copy.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap_or(std::cmp::Ordering::Equal));
    copy
}

fn ordered_alphas(stops: &[AlphaStop]) -> Vec<AlphaStop> {
    let mut copy: Vec<AlphaStop> = stops
        .iter()
        .copied()
        .map(|mut stop| {
            stop.time = finite(stop.time);
            stop.alpha = finite(stop.alpha);
            stop
        })
        .collect();
    copy.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap_or(std::cmp::Ordering::Equal));
    copy
}

fn sample_sorted(times: &[f32], values: &[[f32; 3]], t: f32) -> [f32; 3] {
    if times.is_empty() {
        return [1.0, 1.0, 1.0];
    }
    if times.len() == 1 || !t.is_finite() || t <= times[0] {
        return values[0];
    }
    let last = times.len() - 1;
    if t >= times[last] {
        return values[last];
    }
    let right = times.iter().position(|time| *time >= t).unwrap_or(last);
    if right == 0 {
        return values[0];
    }
    let span = times[right] - times[right - 1];
    if span <= 1.0e-8 {
        return values[right];
    }
    let u = ((t - times[right - 1]) / span).clamp(0.0, 1.0);
    let mut out = [0.0; 3];
    for i in 0..3 {
        out[i] = values[right - 1][i] + (values[right][i] - values[right - 1][i]) * u;
    }
    out
}

fn sample_sorted_scalar(times: &[f32], values: &[f32], t: f32) -> f32 {
    if times.is_empty() {
        return 1.0;
    }
    if times.len() == 1 || !t.is_finite() || t <= times[0] {
        return values[0];
    }
    let last = times.len() - 1;
    if t >= times[last] {
        return values[last];
    }
    let right = times.iter().position(|time| *time >= t).unwrap_or(last);
    if right == 0 {
        return values[0];
    }
    let span = times[right] - times[right - 1];
    if span <= 1.0e-8 {
        return values[right];
    }
    let u = ((t - times[right - 1]) / span).clamp(0.0, 1.0);
    values[right - 1] + (values[right] - values[right - 1]) * u
}

/// Colour at `t`, red green blue and alpha. Colour stops and alpha stops blend on their own.
/// Empty colour stops are white. Empty alpha stops are opaque. Channels are clamped to 0..1.
pub fn sample_gradient(gradient: &Gradient, t: f32) -> [f32; 4] {
    let colors = ordered_colors(&gradient.colors);
    let alphas = ordered_alphas(&gradient.alphas);
    let times: Vec<f32> = colors.iter().map(|stop| stop.time).collect();
    let rgb: Vec<[f32; 3]> = colors.iter().map(|stop| stop.rgb).collect();
    let atimes: Vec<f32> = alphas.iter().map(|stop| stop.time).collect();
    let avalues: Vec<f32> = alphas.iter().map(|stop| stop.alpha).collect();
    let rgb = sample_sorted(&times, &rgb, t);
    let alpha = sample_sorted_scalar(&atimes, &avalues, t);
    [rgb[0].clamp(0.0, 1.0), rgb[1].clamp(0.0, 1.0), rgb[2].clamp(0.0, 1.0), alpha.clamp(0.0, 1.0)]
}

pub fn add_color_stop(gradient: &mut Gradient, time: f32, rgb: [f32; 3]) -> usize {
    gradient.colors.push(ColorStop {
        time: finite(time),
        rgb: [finite(rgb[0]), finite(rgb[1]), finite(rgb[2])],
    });
    gradient.colors.len() - 1
}

pub fn add_alpha_stop(gradient: &mut Gradient, time: f32, alpha: f32) -> usize {
    gradient.alphas.push(AlphaStop { time: finite(time), alpha: finite(alpha) });
    gradient.alphas.len() - 1
}

pub fn move_color_stop(gradient: &mut Gradient, index: usize, time: f32) -> bool {
    let Some(stop) = gradient.colors.get_mut(index) else {
        return false;
    };
    stop.time = finite(time);
    true
}

pub fn move_alpha_stop(gradient: &mut Gradient, index: usize, time: f32) -> bool {
    let Some(stop) = gradient.alphas.get_mut(index) else {
        return false;
    };
    stop.time = finite(time);
    true
}

pub fn delete_color_stop(gradient: &mut Gradient, index: usize) -> bool {
    if index >= gradient.colors.len() {
        return false;
    }
    gradient.colors.remove(index);
    true
}

pub fn delete_alpha_stop(gradient: &mut Gradient, index: usize) -> bool {
    if index >= gradient.alphas.len() {
        return false;
    }
    gradient.alphas.remove(index);
    true
}

pub fn set_color_rgb(gradient: &mut Gradient, index: usize, rgb: [f32; 3]) -> bool {
    let Some(stop) = gradient.colors.get_mut(index) else {
        return false;
    };
    stop.rgb = [finite(rgb[0]), finite(rgb[1]), finite(rgb[2])];
    true
}

pub fn set_alpha(gradient: &mut Gradient, index: usize, alpha: f32) -> bool {
    let Some(stop) = gradient.alphas.get_mut(index) else {
        return false;
    };
    stop.alpha = finite(alpha);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1.0e-5
    }

    #[test]
    fn ends_between_and_a_single_key() {
        let curve = Curve::linear(0.0, 10.0);
        assert!(close(evaluate(&curve, 0.0), 0.0));
        assert!(close(evaluate(&curve, 1.0), 10.0));
        assert!(close(evaluate(&curve, 0.25), 2.5));
        assert!(close(evaluate(&curve, -2.0), 0.0));
        assert!(close(evaluate(&curve, 4.0), 10.0));
        let one = Curve::constant(3.5);
        assert!(close(evaluate(&one, 0.0), 3.5));
        assert!(close(evaluate(&one, 0.7), 3.5));
        assert!(close(evaluate(&Curve { keys: vec![] }, 0.4), 0.0));
    }

    #[test]
    fn unsorted_keys_are_sorted_before_sampling() {
        let curve = Curve {
            keys: vec![
                CurveKey::new(1.0, 10.0, CurveEase::Linear),
                CurveKey::new(0.0, 0.0, CurveEase::Linear),
            ],
        };
        assert!(close(evaluate(&curve, 0.25), 2.5));
    }

    #[test]
    fn easing_and_tangents_change_the_middle() {
        let mut curve = Curve::linear(0.0, 10.0);
        curve.keys[0].ease = CurveEase::EaseIn;
        assert!(close(evaluate(&curve, 0.5), 2.5));
        curve.keys[0].ease = CurveEase::Constant;
        assert!(close(evaluate(&curve, 0.5), 0.0));
        assert!(close(evaluate(&curve, 1.0), 10.0));
        curve.keys[0].ease = CurveEase::Tangent;
        curve.keys[0].out_tangent = 0.0;
        curve.keys[1].in_tangent = 0.0;
        assert!(close(evaluate(&curve, 0.5), 5.0));
        apply_preset(&mut curve, CurvePreset::EaseInOut);
        let mid = evaluate(&curve, 0.5);
        assert!(mid > 4.0 && mid < 6.0);
    }

    #[test]
    fn gradient_blends_colour_and_alpha_apart() {
        let mut gradient = Gradient::from_endpoints([1.0, 0.0, 0.0, 0.0], [0.0, 0.0, 1.0, 1.0]);
        gradient.colors.reverse();
        gradient.alphas.reverse();
        let mid = sample_gradient(&gradient, 0.5);
        assert!(close(mid[0], 0.5) && close(mid[2], 0.5) && close(mid[1], 0.0));
        assert!(close(mid[3], 0.5));
        let one = Gradient {
            colors: vec![ColorStop { time: 0.2, rgb: [0.0, 1.0, 0.0] }],
            alphas: vec![AlphaStop { time: 0.2, alpha: 0.25 }],
        };
        let sample = sample_gradient(&one, 0.9);
        assert!(close(sample[1], 1.0) && close(sample[3], 0.25));
    }
}
