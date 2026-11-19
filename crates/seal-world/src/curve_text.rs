//! Curve and gradient values stored as SealText.
//!
//! A damaged value does not parse. Callers receive [`Curve::fallback`] or
//! [`Gradient::fallback`] plus an error message, and keep running.

use crate::curve::{AlphaStop, ColorStop, Curve, CurveEase, CurveKey, Gradient};
use seal_sealtext::{SealValue, TextError};

fn num(value: f32) -> String {
    if !value.is_finite() {
        return "0".to_string();
    }
    // Shortest text that reads back to the same f32: 0.4 stays "0.4", not "0.40000001".
    let mut text = format!("{value}");
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    if text.is_empty() || text == "-" || text == "-0" {
        "0".to_string()
    } else {
        text
    }
}

fn number(value: f32) -> SealValue {
    SealValue::Number(num(value))
}

fn ease_name(ease: CurveEase) -> &'static str {
    match ease {
        CurveEase::Tangent => "tangent",
        CurveEase::Constant => "constant",
        CurveEase::Linear => "linear",
        CurveEase::EaseIn => "ease_in",
        CurveEase::EaseOut => "ease_out",
        CurveEase::EaseInOut => "ease_in_out",
    }
}

fn ease_from(text: &str) -> Option<CurveEase> {
    Some(match text {
        "tangent" => CurveEase::Tangent,
        "constant" => CurveEase::Constant,
        "linear" => CurveEase::Linear,
        "ease_in" => CurveEase::EaseIn,
        "ease_out" => CurveEase::EaseOut,
        "ease_in_out" => CurveEase::EaseInOut,
        _ => return None,
    })
}

fn as_f32(value: &SealValue) -> Option<f32> {
    match value {
        SealValue::Number(text) => text.parse().ok(),
        _ => None,
    }
}

fn damaged(kind: &str, detail: &str) -> String {
    format!("{kind} text is damaged: {detail}")
}

fn parse_error(kind: &str, error: &TextError) -> String {
    damaged(kind, &error.to_string())
}

/// Write `curve` as one SealText value.
pub fn curve_to_text(curve: &Curve) -> String {
    let keys: Vec<SealValue> = curve
        .keys
        .iter()
        .map(|key| {
            SealValue::Map(seal_sealtext::SealMap::new(vec![
                ("time".into(), number(key.time)),
                ("value".into(), number(key.value)),
                ("in".into(), number(key.in_tangent)),
                ("out".into(), number(key.out_tangent)),
                ("ease".into(), SealValue::String(ease_name(key.ease).into())),
            ]))
        })
        .collect();
    let value =
        SealValue::Map(seal_sealtext::SealMap::new(vec![("keys".into(), SealValue::Array(keys))]));
    compact(&value.to_text())
}

/// Write `gradient` as one SealText value.
pub fn gradient_to_text(gradient: &Gradient) -> String {
    let colors: Vec<SealValue> = gradient
        .colors
        .iter()
        .map(|stop| {
            SealValue::Map(seal_sealtext::SealMap::new(vec![
                ("time".into(), number(stop.time)),
                ("r".into(), number(stop.rgb[0])),
                ("g".into(), number(stop.rgb[1])),
                ("b".into(), number(stop.rgb[2])),
            ]))
        })
        .collect();
    let alphas: Vec<SealValue> = gradient
        .alphas
        .iter()
        .map(|stop| {
            SealValue::Map(seal_sealtext::SealMap::new(vec![
                ("time".into(), number(stop.time)),
                ("a".into(), number(stop.alpha)),
            ]))
        })
        .collect();
    let value = SealValue::Map(seal_sealtext::SealMap::new(vec![
        ("colors".into(), SealValue::Array(colors)),
        ("alphas".into(), SealValue::Array(alphas)),
    ]));
    compact(&value.to_text())
}

/// SealText's writer breaks maps across lines. Effect files keep one value on one line.
fn compact(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        if ch == '\n' || ch == '\r' {
            if !out.ends_with(' ') && !out.is_empty() {
                out.push(' ');
            }
        } else {
            out.push(ch);
        }
    }
    out.trim().to_string()
}

/// Parse a curve. Damaged text returns the fallback curve and an error message.
pub fn load_curve(text: &str) -> (Curve, Option<String>) {
    let value = match seal_sealtext::parse(text) {
        Ok(value) => value,
        Err(error) => return (Curve::fallback(), Some(parse_error("curve", &error))),
    };
    match curve_from_value(&value) {
        Ok(curve) => (curve, None),
        Err(error) => (Curve::fallback(), Some(damaged("curve", &error))),
    }
}

/// Parse a gradient. Damaged text returns the fallback gradient and an error message.
pub fn load_gradient(text: &str) -> (Gradient, Option<String>) {
    let value = match seal_sealtext::parse(text) {
        Ok(value) => value,
        Err(error) => return (Gradient::fallback(), Some(parse_error("gradient", &error))),
    };
    match gradient_from_value(&value) {
        Ok(gradient) => (gradient, None),
        Err(error) => (Gradient::fallback(), Some(damaged("gradient", &error))),
    }
}

fn curve_from_value(value: &SealValue) -> Result<Curve, String> {
    let keys = value
        .get("keys")
        .and_then(|item| match item {
            SealValue::Array(items) => Some(items),
            _ => None,
        })
        .ok_or_else(|| "keys must be a list".to_string())?;
    let mut parsed = Vec::with_capacity(keys.len());
    for key in keys {
        let time =
            key.get("time").and_then(as_f32).ok_or_else(|| "a key needs a time".to_string())?;
        let amount =
            key.get("value").and_then(as_f32).ok_or_else(|| "a key needs a value".to_string())?;
        let in_tangent = key.get("in").and_then(as_f32).unwrap_or(0.0);
        let out_tangent = key.get("out").and_then(as_f32).unwrap_or(0.0);
        let ease = match key.get("ease") {
            Some(SealValue::String(name)) => {
                ease_from(name).ok_or_else(|| format!("unknown easing {name}"))?
            }
            Some(_) => return Err("easing must be text".into()),
            None => CurveEase::Linear,
        };
        parsed.push(CurveKey { time, value: amount, in_tangent, out_tangent, ease });
    }
    Ok(Curve { keys: parsed })
}

fn gradient_from_value(value: &SealValue) -> Result<Gradient, String> {
    let colors = value
        .get("colors")
        .and_then(|item| match item {
            SealValue::Array(items) => Some(items),
            _ => None,
        })
        .ok_or_else(|| "colors must be a list".to_string())?;
    let alphas = value
        .get("alphas")
        .and_then(|item| match item {
            SealValue::Array(items) => Some(items),
            _ => None,
        })
        .ok_or_else(|| "alphas must be a list".to_string())?;
    let mut color_stops = Vec::with_capacity(colors.len());
    for stop in colors {
        let time = stop
            .get("time")
            .and_then(as_f32)
            .ok_or_else(|| "a colour stop needs a time".to_string())?;
        let r =
            stop.get("r").and_then(as_f32).ok_or_else(|| "a colour stop needs r".to_string())?;
        let g =
            stop.get("g").and_then(as_f32).ok_or_else(|| "a colour stop needs g".to_string())?;
        let b =
            stop.get("b").and_then(as_f32).ok_or_else(|| "a colour stop needs b".to_string())?;
        color_stops.push(ColorStop { time, rgb: [r, g, b] });
    }
    let mut alpha_stops = Vec::with_capacity(alphas.len());
    for stop in alphas {
        let time = stop
            .get("time")
            .and_then(as_f32)
            .ok_or_else(|| "an alpha stop needs a time".to_string())?;
        let alpha =
            stop.get("a").and_then(as_f32).ok_or_else(|| "an alpha stop needs a".to_string())?;
        alpha_stops.push(AlphaStop { time, alpha });
    }
    Ok(Gradient { colors: color_stops, alphas: alpha_stops })
}

/// Insert, replace, or remove one `key values...` line. Other lines stay as they were.
pub fn upsert_line(text: &str, key: &str, value: Option<&str>) -> String {
    let mut out = String::new();
    let mut found = false;
    for line in text.split_inclusive('\n') {
        let raw = line.trim_end_matches(['\n', '\r']);
        let code = raw.split('#').next().unwrap_or("").trim();
        let line_key = code.split_whitespace().next().unwrap_or("");
        if line_key == key {
            found = true;
            if let Some(value) = value {
                out.push_str(key);
                out.push(' ');
                out.push_str(value);
                out.push('\n');
            }
            continue;
        }
        out.push_str(raw);
        out.push('\n');
    }
    if !found {
        if let Some(value) = value {
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(key);
            out.push(' ');
            out.push_str(value);
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curve::{evaluate, sample_gradient, CurveEase, CurveKey};

    #[test]
    fn curve_and_gradient_round_trip() {
        let curve = Curve {
            keys: vec![
                CurveKey {
                    time: 0.25,
                    value: 2.0,
                    in_tangent: 1.0,
                    out_tangent: -1.0,
                    ease: CurveEase::Tangent,
                },
                CurveKey::new(0.0, 0.5, CurveEase::EaseIn),
            ],
        };
        let (back, error) = load_curve(&curve_to_text(&curve));
        assert!(error.is_none(), "{error:?}");
        assert_eq!(back, curve);
        assert!((evaluate(&back, 0.0) - 0.5).abs() < 1.0e-4);
        let gradient = Gradient::from_endpoints([1.0, 0.25, 0.0, 1.0], [0.0, 0.0, 1.0, 0.0]);
        let (back, error) = load_gradient(&gradient_to_text(&gradient));
        assert!(error.is_none(), "{error:?}");
        assert_eq!(back, gradient);
        let mid = sample_gradient(&back, 0.5);
        assert!((mid[0] - 0.5).abs() < 1.0e-4 && (mid[3] - 0.5).abs() < 1.0e-4);
    }

    #[test]
    fn damaged_text_falls_back_and_names_the_problem() {
        let (curve, error) = load_curve("this is not a curve");
        assert_eq!(curve, Curve::fallback());
        let error = error.expect("error");
        assert!(error.contains("damaged"), "{error}");
        let (gradient, error) = load_gradient("{colors = \"no\"}");
        assert_eq!(gradient, Gradient::fallback());
        let error = error.expect("error");
        assert!(error.contains("damaged"), "{error}");
    }
}

#[cfg(test)]
mod num_tests {
    #[test]
    fn numbers_keep_their_short_form() {
        assert_eq!(super::num(0.4), "0.4");
        assert_eq!(super::num(-1.0), "-1");
        assert_eq!(super::num(0.0000001), "0.0000001");
        assert_eq!(super::num(-0.0), "0");
    }
}
