//! Texture coordinates, width curves, colour gradients and end caps for 2D shapes.
//!
//! A filled shape maps its picture across the bounds of its points, then shifts,
//! scales and turns those coordinates. A line samples its width curve and colour
//! gradient by distance along the line. Caps are only built when a cap, a curve
//! or a gradient is set; a plain line keeps the existing stroke.

use crate::curve::{sample_gradient, Curve, Gradient};
use crate::curve_text::{load_curve, load_gradient};

/// How a line ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapKind {
    None,
    Square,
    Round,
}

pub fn cap_kind(text: &str) -> CapKind {
    match text.trim() {
        "square" => CapKind::Square,
        "round" => CapKind::Round,
        _ => CapKind::None,
    }
}

/// A plain line uses the stroke already drawn for it.
pub fn line_is_styled(curve: &str, gradient: &str, cap_begin: &str, cap_end: &str) -> bool {
    !curve.trim().is_empty()
        || !gradient.trim().is_empty()
        || cap_kind(cap_begin) != CapKind::None
        || cap_kind(cap_end) != CapKind::None
}

fn bounds(points: &[[f32; 2]]) -> Option<([f32; 2], [f32; 2])> {
    let mut min = [f32::MAX, f32::MAX];
    let mut max = [f32::MIN, f32::MIN];
    for p in points {
        min[0] = min[0].min(p[0]);
        min[1] = min[1].min(p[1]);
        max[0] = max[0].max(p[0]);
        max[1] = max[1].max(p[1]);
    }
    if !min[0].is_finite() || max[0] - min[0] <= 1.0e-6 || max[1] - min[1] <= 1.0e-6 {
        return None;
    }
    Some((min, max))
}

/// Picture coordinate of `point` inside `points`, after offset, scale and rotation.
///
/// The bounds of `points` map to 0..1. Scale and rotation are around the centre
/// of that square. Offset is added last, in picture widths and heights.
pub fn map_uv(
    point: [f32; 2],
    points: &[[f32; 2]],
    offset: [f32; 2],
    scale: [f32; 2],
    rotation_deg: f32,
) -> [f32; 2] {
    let Some((min, max)) = bounds(points) else {
        return offset;
    };
    let u = (point[0] - min[0]) / (max[0] - min[0]);
    let v = (point[1] - min[1]) / (max[1] - min[1]);
    let (u, v) = (0.5 + (u - 0.5) * scale[0], 0.5 + (v - 0.5) * scale[1]);
    let (sin, cos) = rotation_deg.to_radians().sin_cos();
    let (du, dv) = (u - 0.5, v - 0.5);
    [0.5 + du * cos - dv * sin + offset[0], 0.5 + du * sin + dv * cos + offset[1]]
}

fn curve_of(text: &str) -> Option<Curve> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let (curve, error) = load_curve(text);
    if error.is_some() {
        None
    } else {
        Some(curve)
    }
}

fn gradient_of(text: &str) -> Option<Gradient> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let (gradient, error) = load_gradient(text);
    if error.is_some() {
        None
    } else {
        Some(gradient)
    }
}

fn length(points: &[[f32; 2]]) -> f32 {
    points
        .windows(2)
        .map(|w| {
            let (dx, dy) = (w[1][0] - w[0][0], w[1][1] - w[0][1]);
            (dx * dx + dy * dy).sqrt()
        })
        .sum()
}

/// Point, width and colour at distance fraction `t` (0 at the start, 1 at the end).
///
/// An empty curve keeps `base_width`. A curve's value multiplies `base_width`.
/// An empty gradient keeps `color`.
pub fn sample_at(
    points: &[[f32; 2]],
    t: f32,
    base_width: f32,
    curve_text: &str,
    gradient_text: &str,
    color: [f32; 4],
) -> ([f32; 2], f32, [f32; 4]) {
    let t = t.clamp(0.0, 1.0);
    let total = length(points).max(1.0e-6);
    let mut left = t * total;
    let mut at = *points.first().unwrap_or(&[0.0, 0.0]);
    for w in points.windows(2) {
        let (dx, dy) = (w[1][0] - w[0][0], w[1][1] - w[0][1]);
        let seg = (dx * dx + dy * dy).sqrt();
        if seg <= 1.0e-6 {
            continue;
        }
        if left <= seg {
            let u = left / seg;
            at = [w[0][0] + dx * u, w[0][1] + dy * u];
            break;
        }
        left -= seg;
        at = w[1];
    }
    let width = match curve_of(curve_text) {
        Some(curve) => base_width * crate::curve::evaluate(&curve, t),
        None => base_width,
    };
    let colour = match gradient_of(gradient_text) {
        Some(gradient) => sample_gradient(&gradient, t),
        None => color,
    };
    (at, width.max(0.0), colour)
}

/// One corner of a stroked line, already placed, with its colour.
#[derive(Clone, Copy, Debug)]
pub struct RibbonVert {
    pub at: [f32; 2],
    pub color: [f32; 4],
}

/// Triangles of a line with a varying width, a colour gradient and end caps.
pub fn ribbon(
    points: &[[f32; 2]],
    closed: bool,
    base_width: f32,
    curve_text: &str,
    gradient_text: &str,
    color: [f32; 4],
    cap_begin: CapKind,
    cap_end: CapKind,
) -> (Vec<RibbonVert>, Vec<[usize; 3]>) {
    if points.len() < 2 {
        return (Vec::new(), Vec::new());
    }
    let mut ring: Vec<[f32; 2]> = points.to_vec();
    if closed && ring.len() > 2 {
        let (a, b) = (ring[0], *ring.last().unwrap());
        let (dx, dy) = (a[0] - b[0], a[1] - b[1]);
        if dx * dx + dy * dy > 1.0e-8 {
            ring.push(ring[0]);
        }
    }
    let total = length(&ring).max(1.0e-6);
    let mut samples = Vec::new();
    let mut walked = 0.0;
    for (i, p) in ring.iter().enumerate() {
        if i > 0 {
            let (dx, dy) = (p[0] - ring[i - 1][0], p[1] - ring[i - 1][1]);
            walked += (dx * dx + dy * dy).sqrt();
        }
        let t = (walked / total).clamp(0.0, 1.0);
        let (_, width, colour) = sample_at(&ring, t, base_width, curve_text, gradient_text, color);
        samples.push((*p, width, colour));
    }
    let dirs = directions(&ring);
    let mut verts = Vec::new();
    let mut tris = Vec::new();
    let mut left_i = Vec::new();
    let mut right_i = Vec::new();
    for (i, (p, width, colour)) in samples.iter().enumerate() {
        let n = perp(dirs[i]);
        let hw = width * 0.5;
        left_i.push(verts.len());
        verts.push(RibbonVert { at: [p[0] + n[0] * hw, p[1] + n[1] * hw], color: *colour });
        right_i.push(verts.len());
        verts.push(RibbonVert { at: [p[0] - n[0] * hw, p[1] - n[1] * hw], color: *colour });
    }
    for i in 0..left_i.len().saturating_sub(1) {
        tris.push([left_i[i], right_i[i], left_i[i + 1]]);
        tris.push([right_i[i], right_i[i + 1], left_i[i + 1]]);
    }
    if !closed {
        add_cap(&mut verts, &mut tris, &samples, &dirs, 0, cap_begin, true);
        add_cap(&mut verts, &mut tris, &samples, &dirs, samples.len() - 1, cap_end, false);
    }
    (verts, tris)
}

fn add_cap(
    verts: &mut Vec<RibbonVert>,
    tris: &mut Vec<[usize; 3]>,
    samples: &[([f32; 2], f32, [f32; 4])],
    dirs: &[[f32; 2]],
    index: usize,
    kind: CapKind,
    begin: bool,
) {
    if kind == CapKind::None || samples.is_empty() {
        return;
    }
    let (p, width, colour) = samples[index];
    let forward = if begin { [-dirs[index][0], -dirs[index][1]] } else { dirs[index] };
    let n = perp(if begin { [-forward[0], -forward[1]] } else { forward });
    let hw = width * 0.5;
    let centre = verts.len();
    verts.push(RibbonVert { at: p, color: colour });
    match kind {
        CapKind::None => {}
        CapKind::Square => {
            let tip = [p[0] + forward[0] * hw, p[1] + forward[1] * hw];
            let a = verts.len();
            verts.push(RibbonVert { at: [tip[0] + n[0] * hw, tip[1] + n[1] * hw], color: colour });
            let b = verts.len();
            verts.push(RibbonVert { at: [tip[0] - n[0] * hw, tip[1] - n[1] * hw], color: colour });
            tris.push([centre, a, b]);
        }
        CapKind::Round => {
            let steps = 8;
            let mut prev = None;
            for s in 0..=steps {
                let ang =
                    std::f32::consts::PI * (s as f32) / steps as f32 - std::f32::consts::FRAC_PI_2;
                let (sin, cos) = ang.sin_cos();
                let at = [
                    p[0] + (forward[0] * cos + n[0] * sin) * hw,
                    p[1] + (forward[1] * cos + n[1] * sin) * hw,
                ];
                let id = verts.len();
                verts.push(RibbonVert { at, color: colour });
                if let Some(earlier) = prev {
                    tris.push([centre, earlier, id]);
                }
                prev = Some(id);
            }
        }
    }
}

fn perp(d: [f32; 2]) -> [f32; 2] {
    [-d[1], d[0]]
}

fn norm(d: [f32; 2]) -> [f32; 2] {
    let len = (d[0] * d[0] + d[1] * d[1]).sqrt();
    if len <= 1.0e-6 {
        [1.0, 0.0]
    } else {
        [d[0] / len, d[1] / len]
    }
}

fn directions(points: &[[f32; 2]]) -> Vec<[f32; 2]> {
    let n = points.len();
    let mut dirs = Vec::with_capacity(n);
    for i in 0..n {
        let prev = if i == 0 { points[0] } else { points[i - 1] };
        let next = if i + 1 == n { points[i] } else { points[i + 1] };
        let d = if i == 0 {
            [next[0] - points[0][0], next[1] - points[0][1]]
        } else if i + 1 == n {
            [points[i][0] - prev[0], points[i][1] - prev[1]]
        } else {
            let a = norm([points[i][0] - prev[0], points[i][1] - prev[1]]);
            let b = norm([next[0] - points[i][0], next[1] - points[i][1]]);
            [a[0] + b[0], a[1] + b[1]]
        };
        dirs.push(norm(d));
    }
    dirs
}

/// Vertices of a round or square cap past the end of a straight segment.
///
/// `along` points from the start of the segment toward `end`. The returned
/// points include the cap's furthest point, which sits half the width past
/// `end` for a square or round cap and on `end` for none.
pub fn cap_extent(end: [f32; 2], along: [f32; 2], width: f32, kind: CapKind) -> Vec<[f32; 2]> {
    let dir = norm(along);
    let n = perp(dir);
    let hw = width * 0.5;
    let side = |s: f32| [end[0] + n[0] * hw * s, end[1] + n[1] * hw * s];
    match kind {
        CapKind::None => vec![side(1.0), side(-1.0)],
        CapKind::Square => {
            let tip = [end[0] + dir[0] * hw, end[1] + dir[1] * hw];
            vec![
                side(1.0),
                side(-1.0),
                [tip[0] + n[0] * hw, tip[1] + n[1] * hw],
                [tip[0] - n[0] * hw, tip[1] - n[1] * hw],
            ]
        }
        CapKind::Round => {
            let mut pts = vec![side(1.0), side(-1.0)];
            let steps = 8;
            for s in 0..=steps {
                let ang =
                    std::f32::consts::PI * (s as f32) / steps as f32 - std::f32::consts::FRAC_PI_2;
                let (sin, cos) = ang.sin_cos();
                pts.push([
                    end[0] + (dir[0] * cos + n[0] * sin) * hw,
                    end[1] + (dir[1] * cos + n[1] * sin) * hw,
                ]);
            }
            pts
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curve::Curve;
    use crate::curve::Gradient;
    use crate::curve_text::{curve_to_text, gradient_to_text};

    #[test]
    fn uv_offset_scale_and_rotation() {
        let square = [[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]];
        let id = map_uv([0.0, 0.0], &square, [0.0, 0.0], [1.0, 1.0], 0.0);
        assert!((id[0]).abs() < 1.0e-5 && id[1].abs() < 1.0e-5, "{id:?}");
        let mid = map_uv([1.0, 1.0], &square, [0.25, -0.25], [1.0, 1.0], 0.0);
        assert!((mid[0] - 0.75).abs() < 1.0e-5 && (mid[1] - 0.25).abs() < 1.0e-5, "{mid:?}");
        // Scale 2 around the centre pushes the min corner out to -0.5.
        let scaled = map_uv([0.0, 0.0], &square, [0.0, 0.0], [2.0, 2.0], 0.0);
        assert!((scaled[0] + 0.5).abs() < 1.0e-4 && (scaled[1] + 0.5).abs() < 1.0e-4, "{scaled:?}");
        // A quarter turn around the centre sends the right-middle point to the top.
        let turned = map_uv([2.0, 1.0], &square, [0.0, 0.0], [1.0, 1.0], 90.0);
        assert!((turned[0] - 0.5).abs() < 1.0e-4 && (turned[1] - 1.0).abs() < 1.0e-4, "{turned:?}");
    }

    #[test]
    fn width_curve_and_gradient_sample_along_the_line() {
        let line = [[0.0, 0.0], [4.0, 0.0]];
        let curve = curve_to_text(&Curve::linear(1.0, 3.0));
        let gradient =
            gradient_to_text(&Gradient::from_endpoints([1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0, 1.0]));
        for (t, width, red) in [(0.0, 2.0, 1.0), (0.5, 4.0, 0.5), (1.0, 6.0, 0.0)] {
            let (at, w, colour) = sample_at(&line, t, 2.0, &curve, &gradient, [1.0; 4]);
            assert!((at[0] - t * 4.0).abs() < 1.0e-4 && at[1].abs() < 1.0e-4, "{at:?}");
            assert!((w - width).abs() < 1.0e-3, "width {w} at {t}");
            assert!((colour[0] - red).abs() < 1.0e-3, "red {} at {t}", colour[0]);
            assert!((colour[2] - (1.0 - red)).abs() < 1.0e-3, "blue {} at {t}", colour[2]);
        }
        let plain = sample_at(&line, 0.5, 2.0, "", "", [0.2, 0.4, 0.6, 1.0]);
        assert!((plain.1 - 2.0).abs() < 1.0e-5);
        assert!((plain.2[0] - 0.2).abs() < 1.0e-5);
    }

    #[test]
    fn caps_stop_at_the_end_or_reach_half_a_width_past_it() {
        let end = [2.0, 0.0];
        let along = [1.0, 0.0];
        let none = cap_extent(end, along, 1.0, CapKind::None);
        assert!(none.iter().all(|p| (p[0] - 2.0).abs() < 1.0e-4), "{none:?}");
        let square = cap_extent(end, along, 1.0, CapKind::Square);
        let max_x = square.iter().map(|p| p[0]).fold(0.0, f32::max);
        assert!((max_x - 2.5).abs() < 1.0e-4, "{square:?}");
        let round = cap_extent(end, along, 1.0, CapKind::Round);
        let tip = round.iter().map(|p| p[0]).fold(0.0, f32::max);
        assert!((tip - 2.5).abs() < 1.0e-3, "{round:?}");
        assert!(round.len() > square.len());
    }
}
