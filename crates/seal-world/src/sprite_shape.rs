//! Spline terrain: a cubic curve through control points, a tiled fill, an edge
//! strip, and an optional collider that follows the curve.
//!
//! Smooth points use an outgoing tangent (the incoming tangent is its opposite).
//! Corner points meet at a sharp bend. The curve is a cubic between each pair.

use seal_scene::sprite_shape::{parse_points, points_text, PointList, SplinePoint, SpriteShape};

use crate::phys2d::{pair_text, parse_pair};
use crate::{format_num, parse_tint, tint_hex, Entity};

pub use seal_scene::sprite_shape::{SpriteShape as Shape, CORNERS};

/// Inspector fields, in order.
pub const KEYS: &[&str] = &[
    "ss_points",
    "ss_closed",
    "ss_fill",
    "ss_fill_scale",
    "ss_edge_top",
    "ss_edge_side",
    "ss_edge_bottom",
    "ss_thickness",
    "ss_corner",
    "ss_layer",
    "ss_order",
    "ss_tint",
    "ss_top",
    "ss_side",
    "ss_bottom",
    "ss_collider",
    "ss_tolerance",
];

/// Which edge picture a segment uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Band {
    Top = 0,
    Side = 1,
    Bottom = 2,
}

/// One filled triangle in the object's own units.
#[derive(Clone, Copy, Debug)]
pub struct FillTri {
    pub pos: [[f32; 2]; 3],
    pub uv: [[f32; 2]; 3],
}

/// One triangle of the edge strip.
#[derive(Clone, Copy, Debug)]
pub struct EdgeTri {
    pub band: Band,
    pub pos: [[f32; 2]; 3],
    pub uv: [[f32; 2]; 3],
}

/// The sampled curve. `seg[i]` is the control segment that arrives at `points[i + 1]`.
#[derive(Clone, Debug, PartialEq)]
pub struct Polyline {
    pub points: Vec<[f32; 2]>,
    pub seg: Vec<usize>,
}

fn yes(on: bool) -> String {
    if on { "yes" } else { "no" }.into()
}

fn num_ok(text: &str, lo: f32, hi: f32) -> bool {
    text.trim().parse::<f32>().is_ok_and(|n| n.is_finite() && (lo..=hi).contains(&n))
}

/// A new sprite shape: a closed bump the edge and the fill can both show.
pub fn starter() -> SpriteShape {
    let points = parse_points(
        "-2 -0.45 corner; -0.8 0.15 smooth 0.7 0.35; 0.8 0.15 smooth 0.7 -0.35; 2 -0.45 corner; 2 -1 corner; -2 -1 corner",
    )
    .expect("starter points");
    SpriteShape {
        on: true,
        points: PointList(points),
        closed: true,
        collider: true,
        ..SpriteShape::default()
    }
}

pub fn shape_fields(e: &Entity) -> Vec<(String, String)> {
    if !e.sprite_shape.on {
        return Vec::new();
    }
    KEYS.iter()
        .filter_map(|key| field_text(e, key).map(|text| ((*key).to_string(), text)))
        .collect()
}

pub fn field_text(e: &Entity, key: &str) -> Option<String> {
    let s = &e.sprite_shape;
    Some(match key {
        "ss_points" => points_text(&s.points.0),
        "ss_closed" => yes(s.closed),
        "ss_fill" => s.fill.clone(),
        "ss_fill_scale" => pair_text(&s.fill_scale),
        "ss_edge_top" => s.edge_top.clone(),
        "ss_edge_side" => s.edge_side.clone(),
        "ss_edge_bottom" => s.edge_bottom.clone(),
        "ss_thickness" => format_num(s.thickness),
        "ss_corner" => s.corner.clone(),
        "ss_layer" => s.layer.to_string(),
        "ss_order" => s.order.to_string(),
        "ss_tint" => tint_hex(&s.tint),
        "ss_top" => pair_text(&s.top_angles),
        "ss_side" => pair_text(&s.side_angles),
        "ss_bottom" => pair_text(&s.bottom_angles),
        "ss_collider" => yes(s.collider),
        "ss_tolerance" => format_num(s.tolerance),
        _ => return None,
    })
}

/// Why a field's text is refused, or `None` when it is fine.
pub fn check(key: &str, value: &str) -> Option<&'static str> {
    let v = value.trim();
    let pair_scale = parse_pair(v).is_some_and(|p| p[0] >= 0.05 && p[1] >= 0.05);
    let pair_any = parse_pair(v).is_some();
    let ok = match key {
        "ss_points" => parse_points(v).is_ok(),
        "ss_closed" | "ss_collider" => matches!(v, "yes" | "no"),
        "ss_fill" | "ss_edge_top" | "ss_edge_side" | "ss_edge_bottom" => !v.contains('\n'),
        "ss_fill_scale" => pair_scale,
        "ss_thickness" => num_ok(v, 0.001, 100.0),
        "ss_corner" => CORNERS.contains(&v),
        "ss_layer" => num_ok(v, -100.0, 100.0),
        "ss_order" => num_ok(v, -10000.0, 10000.0),
        "ss_tint" => parse_tint(v).is_some(),
        "ss_top" | "ss_side" | "ss_bottom" => pair_any,
        "ss_tolerance" => num_ok(v, 0.001, 10.0),
        _ => false,
    };
    if ok {
        return None;
    }
    Some(match key {
        "ss_points" => "inspector-ss-points-invalid",
        "ss_fill_scale" => "inspector-ss-scale-invalid",
        "ss_top" | "ss_side" | "ss_bottom" => "inspector-ss-angles-invalid",
        "ss_thickness" => "inspector-ss-thickness-invalid",
        "ss_tolerance" => "inspector-ss-tolerance-invalid",
        _ => "inspector-value-out-of-range",
    })
}

pub fn apply(e: &mut Entity, key: &str, value: &str) -> bool {
    if !KEYS.contains(&key) || check(key, value).is_some() {
        return false;
    }
    let v = value.trim();
    let s = &mut e.sprite_shape;
    match key {
        "ss_points" => s.points = PointList(parse_points(v).unwrap_or_default()),
        "ss_closed" => s.closed = v == "yes",
        "ss_fill" => s.fill = v.into(),
        "ss_fill_scale" => s.fill_scale = parse_pair(v).unwrap_or(s.fill_scale),
        "ss_edge_top" => s.edge_top = v.into(),
        "ss_edge_side" => s.edge_side = v.into(),
        "ss_edge_bottom" => s.edge_bottom = v.into(),
        "ss_thickness" => s.thickness = v.parse().unwrap_or(s.thickness),
        "ss_corner" => s.corner = v.into(),
        "ss_layer" => s.layer = v.parse::<f32>().map_or(s.layer, |n| n.round() as i32),
        "ss_order" => s.order = v.parse::<f32>().map_or(s.order, |n| n.round() as i32),
        "ss_tint" => s.tint = parse_tint(v).unwrap_or(s.tint),
        "ss_top" => s.top_angles = parse_pair(v).unwrap_or(s.top_angles),
        "ss_side" => s.side_angles = parse_pair(v).unwrap_or(s.side_angles),
        "ss_bottom" => s.bottom_angles = parse_pair(v).unwrap_or(s.bottom_angles),
        "ss_collider" => s.collider = v == "yes",
        "ss_tolerance" => s.tolerance = v.parse().unwrap_or(s.tolerance),
        _ => return false,
    }
    sync_collider(e);
    true
}

fn add(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] + b[0], a[1] + b[1]]
}

fn sub(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn mul(a: [f32; 2], k: f32) -> [f32; 2] {
    [a[0] * k, a[1] * k]
}

fn lerp(a: [f32; 2], b: [f32; 2], t: f32) -> [f32; 2] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

fn len(a: [f32; 2]) -> f32 {
    (a[0] * a[0] + a[1] * a[1]).sqrt()
}

fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    len(sub(b, a))
}

fn incoming(p: &SplinePoint) -> [f32; 2] {
    if p.smooth {
        [-p.out[0], -p.out[1]]
    } else {
        [0.0, 0.0]
    }
}

fn outgoing(p: &SplinePoint) -> [f32; 2] {
    if p.smooth {
        p.out
    } else {
        [0.0, 0.0]
    }
}

/// The four cubic controls of the segment from `a` to `b`.
pub fn segment_controls(a: &SplinePoint, b: &SplinePoint) -> [[f32; 2]; 4] {
    [a.pos, add(a.pos, outgoing(a)), add(b.pos, incoming(b)), b.pos]
}

/// Point at `t` (0 to 1) on a cubic.
pub fn cubic(p: [[f32; 2]; 4], t: f32) -> [f32; 2] {
    let u = 1.0 - t;
    let (b0, b1, b2, b3) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    add(add(mul(p[0], b0), mul(p[1], b1)), add(mul(p[2], b2), mul(p[3], b3)))
}

/// Where the spline is `t` of the way along control segment `seg`.
pub fn curve_at(shape: &SpriteShape, seg: usize, t: f32) -> Option<[f32; 2]> {
    let pts = &shape.points.0;
    let n = pts.len();
    let count = segment_count(shape);
    if seg >= count || n == 0 {
        return None;
    }
    let b = if shape.closed { (seg + 1) % n } else { seg + 1 };
    Some(cubic(segment_controls(&pts[seg], &pts[b]), t.clamp(0.0, 1.0)))
}

pub fn segment_count(shape: &SpriteShape) -> usize {
    let n = shape.points.0.len();
    if n < 2 {
        0
    } else if shape.closed {
        n
    } else {
        n - 1
    }
}

fn split_cubic(p: [[f32; 2]; 4]) -> ([[f32; 2]; 4], [[f32; 2]; 4]) {
    let mid = |a: [f32; 2], b: [f32; 2]| lerp(a, b, 0.5);
    let p01 = mid(p[0], p[1]);
    let p12 = mid(p[1], p[2]);
    let p23 = mid(p[2], p[3]);
    let p012 = mid(p01, p12);
    let p123 = mid(p12, p23);
    let p0123 = mid(p012, p123);
    ([p[0], p01, p012, p0123], [p0123, p123, p23, p[3]])
}

fn dist_to_chord(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let ab = sub(b, a);
    let den = ab[0] * ab[0] + ab[1] * ab[1];
    if den < 1e-12 {
        return dist(p, a);
    }
    let t = ((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / den;
    dist(p, lerp(a, b, t.clamp(0.0, 1.0)))
}

fn flat_enough(p: [[f32; 2]; 4], tol: f32) -> bool {
    dist_to_chord(p[1], p[0], p[3]) <= tol && dist_to_chord(p[2], p[0], p[3]) <= tol
}

fn tessellate_cubic(p: [[f32; 2]; 4], tol: f32, depth: u32, seg: usize, out: &mut Polyline) {
    if depth >= 8 || flat_enough(p, tol) {
        out.points.push(p[3]);
        out.seg.push(seg);
        return;
    }
    let (left, right) = split_cubic(p);
    tessellate_cubic(left, tol, depth + 1, seg, out);
    tessellate_cubic(right, tol, depth + 1, seg, out);
}

/// Sample the spline. A closed curve repeats its first point at the end.
pub fn tessellate(shape: &SpriteShape) -> Polyline {
    let tol = shape.tolerance.clamp(0.001, 10.0);
    let pts = &shape.points.0;
    let mut out = Polyline { points: Vec::new(), seg: Vec::new() };
    if pts.is_empty() {
        return out;
    }
    out.points.push(pts[0].pos);
    let n = pts.len();
    let count = segment_count(shape);
    for seg in 0..count {
        let b = if shape.closed { (seg + 1) % n } else { seg + 1 };
        tessellate_cubic(segment_controls(&pts[seg], &pts[b]), tol, 0, seg, &mut out);
    }
    out
}

/// Wrap degrees into the range -180 to 180.
pub fn wrap_deg(angle: f32) -> f32 {
    if !angle.is_finite() {
        return 0.0;
    }
    let mut x = angle % 360.0;
    if x > 180.0 {
        x -= 360.0;
    }
    if x <= -180.0 {
        x += 360.0;
    }
    x
}

/// Whether `angle` sits in `lo..=hi`. When `lo` is past `hi` the range wraps.
pub fn in_range(angle: f32, lo: f32, hi: f32) -> bool {
    let (a, lo, hi) = (wrap_deg(angle), wrap_deg(lo), wrap_deg(hi));
    if (lo - hi).abs() < 1e-4 {
        return (a - lo).abs() < 1e-3;
    }
    if lo <= hi {
        (lo..=hi).contains(&a)
    } else {
        a >= lo || a <= hi
    }
}

/// Direction of the segment from `a` to `b`, in degrees.
pub fn segment_angle(a: [f32; 2], b: [f32; 2]) -> f32 {
    let d = sub(b, a);
    wrap_deg(d[1].atan2(d[0]).to_degrees())
}

/// Top, side or bottom from a segment's direction and the three ranges.
/// Top wins, then bottom, then side. Anything left over is a side.
pub fn classify(angle: f32, top: [f32; 2], side: [f32; 2], bottom: [f32; 2]) -> Band {
    if in_range(angle, top[0], top[1]) {
        Band::Top
    } else if in_range(angle, bottom[0], bottom[1]) {
        Band::Bottom
    } else if in_range(angle, side[0], side[1]) {
        Band::Side
    } else {
        Band::Side
    }
}

pub fn classify_shape(shape: &SpriteShape, a: [f32; 2], b: [f32; 2]) -> Band {
    classify(segment_angle(a, b), shape.top_angles, shape.side_angles, shape.bottom_angles)
}

/// Signed area times two. Positive when the ring runs counter-clockwise.
fn shoelace(pts: &[[f32; 2]]) -> f32 {
    let n = pts.len();
    (0..n)
        .map(|i| {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum()
}

/// Area of a simple polygon.
pub fn polygon_area(pts: &[[f32; 2]]) -> f32 {
    shoelace(pts).abs() * 0.5
}

fn tri_area(t: [[f32; 2]; 3]) -> f32 {
    let (a, b, c) = (t[0], t[1], t[2]);
    ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs() * 0.5
}

/// Area of the triangles from the shared polygon splitter.
pub fn triangulated_area(pts: &[[f32; 2]]) -> f32 {
    crate::shape2d::triangulate(pts)
        .iter()
        .map(|t| tri_area([pts[t[0]], pts[t[1]], pts[t[2]]]))
        .sum()
}

/// The outline without a repeated closing point.
pub fn ring(poly: &Polyline) -> Vec<[f32; 2]> {
    let mut pts = poly.points.clone();
    if pts.len() >= 2 && dist(pts[0], *pts.last().unwrap_or(&pts[0])) < 1e-4 {
        pts.pop();
    }
    pts
}

fn clip_plane(
    poly: &[[f32; 2]],
    inside: impl Fn([f32; 2]) -> bool,
    at: impl Fn([f32; 2], [f32; 2]) -> [f32; 2],
) -> Vec<[f32; 2]> {
    if poly.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut prev = *poly.last().unwrap_or(&poly[0]);
    let mut prev_in = inside(prev);
    for &p in poly {
        let inn = inside(p);
        if inn != prev_in {
            out.push(at(prev, p));
        }
        if inn {
            out.push(p);
        }
        prev = p;
        prev_in = inn;
    }
    out
}

fn clip_rect(poly: &[[f32; 2]], min: [f32; 2], max: [f32; 2]) -> Vec<[f32; 2]> {
    let cross = |a: [f32; 2], b: [f32; 2], da: f32, db: f32| {
        let t = da / (da - db);
        lerp(a, b, t.clamp(0.0, 1.0))
    };
    let x0 = clip_plane(
        poly,
        |p| p[0] >= min[0] - 1e-5,
        |a, b| cross(a, b, min[0] - a[0], min[0] - b[0]),
    );
    let x1 = clip_plane(
        &x0,
        |p| p[0] <= max[0] + 1e-5,
        |a, b| cross(a, b, a[0] - max[0], b[0] - max[0]),
    );
    let y0 = clip_plane(
        &x1,
        |p| p[1] >= min[1] - 1e-5,
        |a, b| cross(a, b, min[1] - a[1], min[1] - b[1]),
    );
    clip_plane(&y0, |p| p[1] <= max[1] + 1e-5, |a, b| cross(a, b, a[1] - max[1], b[1] - max[1]))
}

fn push_fan(cell: [f32; 2], scale: [f32; 2], poly: &[[f32; 2]], out: &mut Vec<FillTri>) {
    if poly.len() < 3 {
        return;
    }
    let uv = |p: [f32; 2]| [(p[0] - cell[0]) / scale[0], (p[1] - cell[1]) / scale[1]];
    for i in 1..poly.len() - 1 {
        let pos = [poly[0], poly[i], poly[i + 1]];
        if tri_area(pos) < 1e-8 {
            continue;
        }
        out.push(FillTri { pos, uv: [uv(pos[0]), uv(pos[1]), uv(pos[2])] });
    }
}

/// Triangles that cover `pts` and tile a picture every `scale` units.
pub fn fill_triangles(pts: &[[f32; 2]], scale: [f32; 2]) -> Vec<FillTri> {
    let scale = [scale[0].max(0.05), scale[1].max(0.05)];
    if pts.len() < 3 {
        return Vec::new();
    }
    let (mut min, mut max) = (pts[0], pts[0]);
    for p in pts {
        min = [min[0].min(p[0]), min[1].min(p[1])];
        max = [max[0].max(p[0]), max[1].max(p[1])];
    }
    let step = [scale[0].max((max[0] - min[0]) / 48.0), scale[1].max((max[1] - min[1]) / 48.0)];
    let mut out = Vec::new();
    for tri in crate::shape2d::triangulate(pts) {
        let tri = [pts[tri[0]], pts[tri[1]], pts[tri[2]]];
        let (mut tmin, mut tmax) = (tri[0], tri[0]);
        for p in tri {
            tmin = [tmin[0].min(p[0]), tmin[1].min(p[1])];
            tmax = [tmax[0].max(p[0]), tmax[1].max(p[1])];
        }
        let x0 = (tmin[0] / step[0]).floor() as i32;
        let x1 = (tmax[0] / step[0]).floor() as i32;
        let y0 = (tmin[1] / step[1]).floor() as i32;
        let y1 = (tmax[1] / step[1]).floor() as i32;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let cell = [x as f32 * step[0], y as f32 * step[1]];
                let clipped = clip_rect(&tri, cell, [cell[0] + step[0], cell[1] + step[1]]);
                push_fan(cell, step, &clipped, &mut out);
            }
        }
    }
    out
}

pub fn fill_area(tris: &[FillTri]) -> f32 {
    tris.iter().map(|t| tri_area(t.pos)).sum()
}

fn normal(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    let d = sub(b, a);
    let l = len(d).max(1e-6);
    [-d[1] / l, d[0] / l]
}

fn push_tri(out: &mut Vec<EdgeTri>, band: Band, pos: [[f32; 2]; 3], uv: [[f32; 2]; 3]) {
    if tri_area(pos) < 1e-8 {
        return;
    }
    out.push(EdgeTri { band, pos, uv });
}

fn push_span(
    out: &mut Vec<EdgeTri>,
    band: Band,
    a: [f32; 2],
    b: [f32; 2],
    n: [f32; 2],
    dist0: f32,
    half: f32,
) {
    let span = dist(a, b);
    if span < 1e-5 {
        return;
    }
    let left = |p| add(p, mul(n, half));
    let right = |p| add(p, mul(n, -half));
    let mut walked = 0.0;
    while walked < span - 1e-4 {
        let u0 = (dist0 + walked).rem_euclid(1.0);
        let room = (1.0 - u0).max(1e-4);
        let step = room.min(span - walked);
        let t0 = walked / span;
        let t1 = (walked + step) / span;
        let p0 = lerp(a, b, t0);
        let p1 = lerp(a, b, t1);
        let u1 = u0 + step;
        push_tri(out, band, [left(p0), right(p0), right(p1)], [[u0, 0.0], [u0, 1.0], [u1, 1.0]]);
        push_tri(out, band, [left(p0), right(p1), left(p1)], [[u0, 0.0], [u1, 1.0], [u1, 0.0]]);
        walked += step;
    }
}

fn line_hit(
    origin: [f32; 2],
    dir: [f32; 2],
    other: [f32; 2],
    other_dir: [f32; 2],
) -> Option<[f32; 2]> {
    let den = dir[0] * other_dir[1] - dir[1] * other_dir[0];
    if den.abs() < 1e-6 {
        return None;
    }
    let d = sub(other, origin);
    let t = (d[0] * other_dir[1] - d[1] * other_dir[0]) / den;
    Some(add(origin, mul(dir, t)))
}

fn corner_join(
    out: &mut Vec<EdgeTri>,
    vertex: [f32; 2],
    n0: [f32; 2],
    n1: [f32; 2],
    half: f32,
    mode: &str,
    band: Band,
    u: f32,
) {
    let cap0 = add(vertex, mul(n0, half));
    let cap1 = add(vertex, mul(n1, half));
    let turn = n0[0] * n1[1] - n0[1] * n1[0];
    if turn.abs() < 1e-4 {
        return;
    }
    // The outer rail is the side the turn leaves behind.
    let (outer0, outer1, sign) = if turn > 0.0 {
        (sub(vertex, mul(n0, half)), sub(vertex, mul(n1, half)), -1.0)
    } else {
        (cap0, cap1, 1.0)
    };
    match mode {
        "round" => {
            let a0 = n0[1].atan2(n0[0]);
            let mut sweep = n1[1].atan2(n1[0]) - a0;
            if sweep > std::f32::consts::PI {
                sweep -= std::f32::consts::TAU;
            }
            if sweep < -std::f32::consts::PI {
                sweep += std::f32::consts::TAU;
            }
            let steps = 4;
            let mut prev = add(vertex, mul(n0, half * sign));
            for s in 1..=steps {
                let ang = a0 + sweep * s as f32 / steps as f32;
                let n = [ang.cos(), ang.sin()];
                let next = add(vertex, mul(n, half * sign));
                push_tri(out, band, [vertex, prev, next], [[u, 0.5], [u, 0.0], [u, 0.0]]);
                prev = next;
            }
        }
        "miter" => {
            let dir0 = [n0[1], -n0[0]];
            let dir1 = [n1[1], -n1[0]];
            let hit =
                line_hit(outer0, dir0, outer1, dir1).filter(|p| dist(*p, vertex) <= half * 4.0);
            match hit {
                Some(p) => push_tri(out, band, [outer0, p, outer1], [[u, 0.0], [u, 0.5], [u, 1.0]]),
                None => {
                    push_tri(out, band, [vertex, outer0, outer1], [[u, 0.5], [u, 0.0], [u, 1.0]])
                }
            }
        }
        _ => push_tri(out, band, [vertex, outer0, outer1], [[u, 0.5], [u, 0.0], [u, 1.0]]),
    }
}

/// Edge strip along `outline`. `bands` has one entry per edge. One texture repeat is one unit.
pub fn edge_strip(
    outline: &[[f32; 2]],
    bands: &[Band],
    thickness: f32,
    corner: &str,
    closed: bool,
) -> Vec<EdgeTri> {
    let n = outline.len();
    if n < 2 || bands.len() + 1 < n {
        return Vec::new();
    }
    let edges = n - 1;
    let half = (thickness * 0.5).max(0.0005);
    let mut out = Vec::new();
    let mut travelled = 0.0;
    for i in 0..edges {
        let (a, b) = (outline[i], outline[i + 1]);
        let span = dist(a, b);
        if span < 1e-5 {
            continue;
        }
        let nrm = normal(a, b);
        push_span(&mut out, bands[i], a, b, nrm, travelled, half);
        let joined = if i + 1 < edges {
            Some(i + 1)
        } else if closed {
            Some(0)
        } else {
            None
        };
        if let Some(j) = joined {
            if bands[i] == bands[j] {
                let next = if j == 0 { outline[1] } else { outline[j + 1] };
                let n1 = normal(b, next);
                corner_join(
                    &mut out,
                    b,
                    nrm,
                    n1,
                    half,
                    corner,
                    bands[i],
                    (travelled + span).rem_euclid(1.0),
                );
            }
        }
        travelled += span;
    }
    out
}

/// Sampled outline, its bands, the tiled fill and the edge strip, in local units.
pub fn build(shape: &SpriteShape) -> (Polyline, Vec<Band>, Vec<FillTri>, Vec<EdgeTri>) {
    let poly = tessellate(shape);
    let mut bands = Vec::new();
    for w in poly.points.windows(2) {
        bands.push(classify_shape(shape, w[0], w[1]));
    }
    let ring = ring(&poly);
    let fill = if shape.closed { fill_triangles(&ring, shape.fill_scale) } else { Vec::new() };
    let edge = edge_strip(&poly.points, &bands, shape.thickness, &shape.corner, shape.closed);
    (poly, bands, fill, edge)
}

fn convex(pts: &[[f32; 2]]) -> bool {
    if pts.len() < 3 {
        return false;
    }
    let mut sign = 0.0;
    for i in 0..pts.len() {
        let (a, b, c) = (pts[i], pts[(i + 1) % pts.len()], pts[(i + 2) % pts.len()]);
        let cross = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
        if cross.abs() < 1e-5 {
            continue;
        }
        if sign == 0.0 {
            sign = cross;
        } else if sign * cross < 0.0 {
            return false;
        }
    }
    true
}

/// Collider outline in local units, and whether it is a closed polygon.
pub fn collider_outline(shape: &SpriteShape) -> (bool, Vec<[f32; 2]>) {
    let poly = tessellate(shape);
    let mut pts = ring(&poly);
    let polygon = shape.closed && convex(&pts) && pts.len() >= 3;
    if shape.closed && !polygon && poly.points.len() >= 2 {
        pts = poly.points;
    }
    (polygon, pts)
}

/// Write or clear the 2D body so play can stand on the spline.
pub fn sync_collider(e: &mut Entity) {
    let shape = &e.sprite_shape;
    if !shape.on || !shape.collider {
        if matches!(e.phys2d.shape.as_str(), "edge" | "polygon") && e.phys2d.body == "fixed" {
            e.phys2d.body.clear();
            e.phys2d.points.clear();
        }
        return;
    }
    let (polygon, pts) = collider_outline(shape);
    if pts.len() < 2 {
        return;
    }
    e.phys2d.body = "fixed".into();
    e.phys2d.shape = if polygon { "polygon".into() } else { "edge".into() };
    e.phys2d.points = pts;
}

fn world_xy(e: &Entity, local: [f32; 2]) -> [f32; 2] {
    let (sin, cos) = e.rotation[2].to_radians().sin_cos();
    let (x, y) = (local[0] * e.scale[0], local[1] * e.scale[1]);
    [e.translation[0] + x * cos - y * sin, e.translation[1] + x * sin + y * cos]
}

fn local_xy(e: &Entity, world: [f32; 2]) -> [f32; 2] {
    let (sin, cos) = e.rotation[2].to_radians().sin_cos();
    let dx = world[0] - e.translation[0];
    let dy = world[1] - e.translation[1];
    let sx = if e.scale[0].abs() < 1e-6 { 1.0 } else { e.scale[0] };
    let sy = if e.scale[1].abs() < 1e-6 { 1.0 } else { e.scale[1] };
    [(dx * cos + dy * sin) / sx, (-dx * sin + dy * cos) / sy]
}

fn offset_world(e: &Entity, local: [f32; 2]) -> [f32; 2] {
    let (sin, cos) = e.rotation[2].to_radians().sin_cos();
    let (x, y) = (local[0] * e.scale[0], local[1] * e.scale[1]);
    [x * cos - y * sin, x * sin + y * cos]
}

fn offset_local(e: &Entity, world: [f32; 2]) -> [f32; 2] {
    let (sin, cos) = e.rotation[2].to_radians().sin_cos();
    let sx = if e.scale[0].abs() < 1e-6 { 1.0 } else { e.scale[0] };
    let sy = if e.scale[1].abs() < 1e-6 { 1.0 } else { e.scale[1] };
    [(world[0] * cos + world[1] * sin) / sx, (-world[0] * sin + world[1] * cos) / sy]
}

/// A local point as the flat view shows it (x to the right, y up, unless flipped).
pub fn to_view(e: &Entity, local: [f32; 2], flip: [f32; 2]) -> [f32; 2] {
    let w = world_xy(e, local);
    [w[0] * flip[0], w[1] * flip[1]]
}

pub fn from_view(e: &Entity, view: [f32; 2], flip: [f32; 2]) -> [f32; 2] {
    let fx = if flip[0].abs() < 1e-6 { 1.0 } else { flip[0] };
    let fy = if flip[1].abs() < 1e-6 { 1.0 } else { flip[1] };
    local_xy(e, [view[0] / fx, view[1] / fy])
}

pub fn handle_tip_view(e: &Entity, index: usize, flip: [f32; 2]) -> Option<[f32; 2]> {
    let p = e.sprite_shape.points.0.get(index)?;
    let origin = to_view(e, p.pos, flip);
    let off = offset_world(e, outgoing(p));
    Some([origin[0] + off[0] * flip[0], origin[1] + off[1] * flip[1]])
}

fn point_mut(e: &mut Entity, index: usize) -> Option<&mut SplinePoint> {
    e.sprite_shape.points.0.get_mut(index)
}

pub fn move_point_view(e: &mut Entity, index: usize, view: [f32; 2], flip: [f32; 2]) -> bool {
    let local = from_view(e, view, flip);
    let Some(p) = point_mut(e, index) else { return false };
    p.pos = local;
    sync_collider(e);
    true
}

pub fn move_handle_view(e: &mut Entity, index: usize, tip: [f32; 2], flip: [f32; 2]) -> bool {
    let origin = {
        let Some(p) = e.sprite_shape.points.0.get(index) else { return false };
        if !p.smooth {
            return false;
        }
        to_view(e, p.pos, flip)
    };
    let fx = if flip[0].abs() < 1e-6 { 1.0 } else { flip[0] };
    let fy = if flip[1].abs() < 1e-6 { 1.0 } else { flip[1] };
    let world = [(tip[0] - origin[0]) / fx, (tip[1] - origin[1]) / fy];
    let local = offset_local(e, world);
    let Some(p) = point_mut(e, index) else { return false };
    p.out = local;
    sync_collider(e);
    true
}

pub fn insert_view(e: &mut Entity, after: usize, view: [f32; 2], flip: [f32; 2]) -> bool {
    let n = e.sprite_shape.points.0.len();
    if after >= n {
        return false;
    }
    let local = from_view(e, view, flip);
    let next = if e.sprite_shape.closed {
        (after + 1) % n
    } else if after + 1 < n {
        after + 1
    } else {
        after
    };
    let toward = sub(e.sprite_shape.points.0[next].pos, local);
    let point = SplinePoint { pos: local, smooth: true, out: mul(toward, 0.25) };
    e.sprite_shape.points.0.insert(after + 1, point);
    sync_collider(e);
    true
}

pub fn delete_point(e: &mut Entity, index: usize) -> bool {
    let min = if e.sprite_shape.closed { 3 } else { 2 };
    if e.sprite_shape.points.0.len() <= min || index >= e.sprite_shape.points.0.len() {
        return false;
    }
    e.sprite_shape.points.0.remove(index);
    sync_collider(e);
    true
}

pub fn set_smooth(e: &mut Entity, index: usize, smooth: bool) -> bool {
    let n = e.sprite_shape.points.0.len();
    if index >= n {
        return false;
    }
    if smooth {
        let prev = e.sprite_shape.points.0[(index + n - 1) % n].pos;
        let next = e.sprite_shape.points.0[(index + 1) % n].pos;
        let out = mul(sub(next, prev), 1.0 / 6.0);
        let p = &mut e.sprite_shape.points.0[index];
        p.smooth = true;
        if len(p.out) < 1e-4 {
            p.out = out;
        }
    } else {
        let p = &mut e.sprite_shape.points.0[index];
        p.smooth = false;
        p.out = [0.0, 0.0];
    }
    sync_collider(e);
    true
}

/// Picture path made absolute when the project folder is known.
pub fn picture_path(root: Option<&std::path::Path>, rel: &str) -> Option<String> {
    if rel.is_empty() {
        return None;
    }
    let raw = std::path::Path::new(rel);
    Some(match (root, raw.is_relative()) {
        (Some(root), true) => root.join(raw).to_string_lossy().into_owned(),
        _ => rel.to_string(),
    })
}

/// Map a local point into the view plane. `axes` are the two world axes on screen.
pub fn project(e: &Entity, local: [f32; 2], axes: [usize; 2], flip: [f32; 2]) -> [f32; 2] {
    let w = world_xy(e, local);
    let world = [w[0], w[1], e.translation[2]];
    [world[axes[0]] * flip[0], world[axes[1]] * flip[1]]
}
