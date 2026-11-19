//! Polygon offset and boolean operations shared by graph nodes and the editor.
//!
//! Points are in order around the outline. A positive offset grows a counter-clockwise
//! outline; a negative offset shrinks it. Merge, clip and intersect work on simple
//! outlines and return one ring. Clip is the first outline with the second cut out of it.

use crate::rt::{Elem, Value, V2};

const EPS: f32 = 1.0e-4;

fn snap(p: [f32; 2]) -> [f32; 2] {
    [(p[0] / EPS).round() * EPS, (p[1] / EPS).round() * EPS]
}

fn sub(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn add(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] + b[0], a[1] + b[1]]
}

fn scale(a: [f32; 2], k: f32) -> [f32; 2] {
    [a[0] * k, a[1] * k]
}

fn dot(a: [f32; 2], b: [f32; 2]) -> f32 {
    a[0] * b[0] + a[1] * b[1]
}

fn cross(a: [f32; 2], b: [f32; 2]) -> f32 {
    a[0] * b[1] - a[1] * b[0]
}

fn dist2(a: [f32; 2], b: [f32; 2]) -> f32 {
    let d = sub(a, b);
    dot(d, d)
}

fn near(a: [f32; 2], b: [f32; 2]) -> bool {
    dist2(a, b) <= EPS * EPS * 4.0
}

/// Signed area. Positive when the outline runs counter-clockwise.
pub fn signed_area(pts: &[[f32; 2]]) -> f32 {
    if pts.len() < 3 {
        return 0.0;
    }
    let mut sum = 0.0;
    for i in 0..pts.len() {
        let a = pts[i];
        let b = pts[(i + 1) % pts.len()];
        sum += a[0] * b[1] - b[0] * a[1];
    }
    sum * 0.5
}

/// Area of a simple outline.
pub fn area(pts: &[[f32; 2]]) -> f32 {
    signed_area(pts).abs()
}

fn clean(pts: &[[f32; 2]]) -> Vec<[f32; 2]> {
    let mut out: Vec<[f32; 2]> = Vec::new();
    for p in pts {
        let p = snap(*p);
        if out.last().is_some_and(|q| near(*q, p)) {
            continue;
        }
        out.push(p);
    }
    if out.len() >= 2 && near(out[0], *out.last().unwrap()) {
        out.pop();
    }
    if signed_area(&out) < 0.0 {
        out.reverse();
    }
    out
}

fn right_normal(d: [f32; 2]) -> [f32; 2] {
    let len = dot(d, d).sqrt();
    if len <= EPS {
        return [0.0, 0.0];
    }
    [d[1] / len, -d[0] / len]
}

fn line_hit(
    origin: [f32; 2],
    dir: [f32; 2],
    other: [f32; 2],
    other_dir: [f32; 2],
) -> Option<[f32; 2]> {
    let den = cross(dir, other_dir);
    if den.abs() <= 1.0e-8 {
        return None;
    }
    let t = cross(sub(other, origin), other_dir) / den;
    Some(snap(add(origin, scale(dir, t))))
}

/// Move each edge of a counter-clockwise outline by `distance` (positive grows it).
/// A shrink that collapses the outline returns no points.
pub fn offset(pts: &[[f32; 2]], distance: f32) -> Vec<[f32; 2]> {
    let src = clean(pts);
    if src.len() < 3 || !distance.is_finite() {
        return Vec::new();
    }
    if distance.abs() <= EPS {
        return src;
    }
    let n = src.len();
    let mut out = Vec::with_capacity(n);
    // Which source corner each new point came from, so a shrink that crosses itself can be rejected.
    let mut from = Vec::with_capacity(n);
    for i in 0..n {
        let prev = src[(i + n - 1) % n];
        let cur = src[i];
        let next = src[(i + 1) % n];
        let d0 = sub(cur, prev);
        let d1 = sub(next, cur);
        let n0 = right_normal(d0);
        let n1 = right_normal(d1);
        if dot(n0, n0) <= EPS || dot(n1, n1) <= EPS {
            continue;
        }
        let p0 = add(prev, scale(n0, distance));
        let p1 = add(cur, scale(n1, distance));
        let hit = line_hit(p0, d0, p1, d1).unwrap_or_else(|| add(cur, scale(n0, distance)));
        let limit = distance.abs() * 4.0;
        if dist2(hit, cur) > limit * limit {
            out.push(snap(add(cur, scale(n0, distance))));
            from.push(i);
            out.push(snap(add(cur, scale(n1, distance))));
            from.push(i);
        } else if out.last().map(|q| !near(*q, hit)).unwrap_or(true) {
            out.push(hit);
            from.push(i);
        }
    }
    // A shrink past the middle reverses an edge: the new side runs opposite the old one.
    let reversed = (0..n).any(|i| {
        let Some(a) = from.iter().rposition(|k| *k == i) else { return false };
        let next_i = (i + 1) % n;
        let Some(b) = from.iter().position(|k| *k == next_i) else { return false };
        dot(sub(out[b], out[a]), sub(src[next_i], src[i])) < -EPS
    });
    if reversed || signed_area(&out) <= EPS {
        return Vec::new();
    }
    let out = clean(&out);
    if out.len() < 3 {
        Vec::new()
    } else {
        out
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BoolOp {
    Merge,
    Clip,
    Intersect,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Where {
    In,
    Out,
    On,
}

fn on_edge(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> bool {
    let ab = sub(b, a);
    let len2 = dot(ab, ab);
    if len2 <= EPS * EPS {
        return near(p, a);
    }
    let t = dot(sub(p, a), ab) / len2;
    if !(-EPS..=1.0 + EPS).contains(&t) {
        return false;
    }
    let closest = add(a, scale(ab, t.clamp(0.0, 1.0)));
    dist2(p, closest) <= EPS * EPS * 4.0
}

fn classify(poly: &[[f32; 2]], p: [f32; 2]) -> Where {
    for i in 0..poly.len() {
        if on_edge(p, poly[i], poly[(i + 1) % poly.len()]) {
            return Where::On;
        }
    }
    let mut inside = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (a, b) = (poly[j], poly[i]);
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let x = (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0];
            if p[0] < x {
                inside = !inside;
            }
        }
        j = i;
    }
    if inside {
        Where::In
    } else {
        Where::Out
    }
}

fn seg_param(a: [f32; 2], b: [f32; 2], p: [f32; 2]) -> f32 {
    let ab = sub(b, a);
    let len2 = dot(ab, ab);
    if len2 <= EPS * EPS {
        0.0
    } else {
        (dot(sub(p, a), ab) / len2).clamp(0.0, 1.0)
    }
}

fn proper_cross(a: [f32; 2], b: [f32; 2], c: [f32; 2], d: [f32; 2]) -> Option<[f32; 2]> {
    let r = sub(b, a);
    let s = sub(d, c);
    let den = cross(r, s);
    if den.abs() <= 1.0e-8 {
        return None;
    }
    let t = cross(sub(c, a), s) / den;
    let u = cross(sub(c, a), r) / den;
    if (EPS..1.0 - EPS).contains(&t) && (EPS..1.0 - EPS).contains(&u) {
        Some(snap(add(a, scale(r, t))))
    } else {
        None
    }
}

fn collinear_cuts(a: [f32; 2], b: [f32; 2], c: [f32; 2], d: [f32; 2]) -> Vec<[f32; 2]> {
    let r = sub(b, a);
    if cross(r, sub(d, c)).abs() > 1.0e-3 || cross(r, sub(c, a)).abs() > 1.0e-3 {
        return Vec::new();
    }
    let len2 = dot(r, r);
    if len2 <= EPS * EPS {
        return Vec::new();
    }
    let project = |p: [f32; 2]| dot(sub(p, a), r) / len2;
    let (t0, t1) = (project(c), project(d));
    let (lo, hi) = (t0.min(t1), t0.max(t1));
    let left = lo.max(0.0);
    let right = hi.min(1.0);
    if right - left <= EPS {
        return Vec::new();
    }
    [left, right]
        .into_iter()
        .filter(|t| (EPS..1.0 - EPS).contains(t))
        .map(|t| snap(add(a, scale(r, t))))
        .collect()
}

fn cuts_on(poly: &[[f32; 2]], other: &[[f32; 2]]) -> Vec<(usize, f32, [f32; 2])> {
    let mut cuts = Vec::new();
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        let mut extra = Vec::new();
        for j in 0..other.len() {
            let (c, d) = (other[j], other[(j + 1) % other.len()]);
            if let Some(p) = proper_cross(a, b, c, d) {
                extra.push(p);
            }
            extra.extend(collinear_cuts(a, b, c, d));
            if on_edge(c, a, b) && !near(c, a) && !near(c, b) {
                extra.push(c);
            }
        }
        for p in extra {
            let t = seg_param(a, b, p);
            if (EPS..1.0 - EPS).contains(&t) {
                cuts.push((i, t, p));
            }
        }
    }
    cuts
}

fn split(poly: &[[f32; 2]], cuts: &[(usize, f32, [f32; 2])]) -> Vec<[f32; 2]> {
    let n = poly.len();
    let mut out = Vec::new();
    for i in 0..n {
        out.push(poly[i]);
        let mut on: Vec<(f32, [f32; 2])> =
            cuts.iter().filter(|c| c.0 == i).map(|c| (c.1, c.2)).collect();
        on.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let mut last = 0.0;
        for (t, p) in on {
            if t - last <= EPS || near(p, poly[i]) || near(p, poly[(i + 1) % n]) {
                continue;
            }
            if out.last().is_some_and(|q| near(*q, p)) {
                continue;
            }
            out.push(p);
            last = t;
        }
    }
    clean(&out)
}

fn keep_segment(
    op: BoolOp,
    from_subject: bool,
    start: [f32; 2],
    end: [f32; 2],
    other: &[[f32; 2]],
) -> Option<([f32; 2], [f32; 2])> {
    let mid = snap([(start[0] + end[0]) * 0.5, (start[1] + end[1]) * 0.5]);
    let place = classify(other, mid);
    let (keep, flip) = match (op, place, from_subject) {
        (BoolOp::Merge, Where::Out, _) => (true, false),
        (BoolOp::Merge, Where::On, true) => (true, false),
        (BoolOp::Intersect, Where::In, _) => (true, false),
        (BoolOp::Intersect, Where::On, true) => (true, false),
        (BoolOp::Clip, Where::Out, true) => (true, false),
        (BoolOp::Clip, Where::In, false) => (true, true),
        _ => (false, false),
    };
    if !keep || near(start, end) {
        return None;
    }
    if flip {
        Some((end, start))
    } else {
        Some((start, end))
    }
}

fn segments(
    poly: &[[f32; 2]],
    other: &[[f32; 2]],
    op: BoolOp,
    subject: bool,
) -> Vec<([f32; 2], [f32; 2])> {
    let mut out = Vec::new();
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        if let Some(seg) = keep_segment(op, subject, a, b, other) {
            out.push(seg);
        }
    }
    out
}

fn key(p: [f32; 2]) -> (i32, i32) {
    ((p[0] / EPS).round() as i32, (p[1] / EPS).round() as i32)
}

fn stitch(mut segs: Vec<([f32; 2], [f32; 2])>) -> Vec<[f32; 2]> {
    if segs.is_empty() {
        return Vec::new();
    }
    segs.sort_by(|a, b| a.0[0].partial_cmp(&b.0[0]).unwrap_or(std::cmp::Ordering::Equal));
    let start = segs[0].0;
    let mut ring = vec![start];
    let limit = segs.len();
    let mut guard = 0;
    while guard < limit + 2 {
        guard += 1;
        let at = *ring.last().unwrap();
        if ring.len() > 2 && near(at, start) {
            break;
        }
        let Some(i) = segs.iter().position(|s| key(s.0) == key(at)) else {
            break;
        };
        let next = segs.remove(i).1;
        if near(next, start) {
            break;
        }
        ring.push(next);
    }
    clean(&ring)
}

fn boolean(subject: &[[f32; 2]], clip: &[[f32; 2]], op: BoolOp) -> Vec<[f32; 2]> {
    let a = clean(subject);
    let b = clean(clip);
    if a.len() < 3 || b.len() < 3 {
        return Vec::new();
    }
    let a = split(&a, &cuts_on(&a, &b));
    let b = split(&b, &cuts_on(&b, &a));
    let mut segs = segments(&a, &b, op, true);
    segs.extend(segments(&b, &a, op, false));
    let ring = stitch(segs);
    if ring.len() < 3 {
        Vec::new()
    } else {
        ring
    }
}

/// Union of two outlines.
pub fn merge(a: &[[f32; 2]], b: &[[f32; 2]]) -> Vec<[f32; 2]> {
    boolean(a, b, BoolOp::Merge)
}

/// `a` with `b` cut out of it.
pub fn clip(a: &[[f32; 2]], b: &[[f32; 2]]) -> Vec<[f32; 2]> {
    boolean(a, b, BoolOp::Clip)
}

/// The overlap of two outlines.
pub fn intersect(a: &[[f32; 2]], b: &[[f32; 2]]) -> Vec<[f32; 2]> {
    boolean(a, b, BoolOp::Intersect)
}

/// Whether every point is inside `ring` or on its edge.
///
/// A union of two separate outlines is two rings. This is how the editor tells
/// that case from one joined outline.
pub fn contains_points(ring: &[[f32; 2]], points: &[[f32; 2]]) -> bool {
    !ring.is_empty() && points.iter().all(|p| !matches!(classify(ring, *p), Where::Out))
}

fn from_values(points: &[Value]) -> Vec<[f32; 2]> {
    points
        .iter()
        .map(|v| {
            let p = v.vec2();
            [p.x as f32, p.y as f32]
        })
        .collect()
}

fn to_values(points: &[[f32; 2]]) -> Vec<Value> {
    points.iter().map(|p| Value::Vec2(V2 { x: p[0] as f64, y: p[1] as f64 })).collect()
}

/// Graph value of an offset outline. `distance` is in the same units as the points.
pub fn offset_values(points: Vec<Value>, distance: f64) -> Vec<Value> {
    to_values(&offset(&from_values(&points), distance as f32))
}

/// Graph value of a union.
pub fn merge_values(a: Vec<Value>, b: Vec<Value>) -> Vec<Value> {
    to_values(&merge(&from_values(&a), &from_values(&b)))
}

/// Graph value of a difference (`a` minus `b`).
pub fn clip_values(a: Vec<Value>, b: Vec<Value>) -> Vec<Value> {
    to_values(&clip(&from_values(&a), &from_values(&b)))
}

/// Graph value of an overlap.
pub fn intersect_values(a: Vec<Value>, b: Vec<Value>) -> Vec<Value> {
    to_values(&intersect(&from_values(&a), &from_values(&b)))
}

/// Element type the polygon nodes return.
pub fn list_elem() -> Elem {
    Elem::Vec2
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(origin: [f32; 2], size: f32) -> Vec<[f32; 2]> {
        let (x, y) = (origin[0], origin[1]);
        vec![[x, y], [x + size, y], [x + size, y + size], [x, y + size]]
    }

    fn close(got: f32, expect: f32) {
        assert!((got - expect).abs() < 1.0e-3, "{got} vs {expect}");
    }

    #[test]
    fn offset_grows_and_shrinks_a_square() {
        let unit = square([0.0, 0.0], 1.0);
        close(area(&unit), 1.0);
        let grown = offset(&unit, 0.5);
        close(area(&grown), 4.0);
        let shrunk = offset(&unit, -0.25);
        close(area(&shrunk), 0.25);
        assert!(offset(&unit, -0.6).is_empty(), "a shrink past the centre disappears");
        close(area(&offset(&unit, 0.0)), 1.0);
    }

    #[test]
    fn boolean_areas_of_overlapping_squares() {
        let a = square([0.0, 0.0], 1.0);
        let b = square([0.5, 0.0], 1.0);
        close(area(&merge(&a, &b)), 1.5);
        close(area(&clip(&a, &b)), 0.5);
        close(area(&intersect(&a, &b)), 0.5);
        let c = square([1.0, 1.0], 2.0);
        let d = square([0.0, 0.0], 2.0);
        close(area(&merge(&d, &c)), 7.0);
        close(area(&intersect(&d, &c)), 1.0);
        close(area(&clip(&d, &c)), 3.0);
    }
}
