//! Physics query nodes shared by the interpreter and compiled graphs.
//!
//! The host lists the colliders it can see. Both back ends call the functions
//! here, so a ray, a sweep and an overlap come out the same.

use crate::rt::{Elem, Host, Value, NO_ENTITY, V2, V3};

/// Which query a [`crate::Op::Phys`] node runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    RaycastAll,
    ShapeCast,
    Point,
    OverlapBox,
    OverlapCircle,
    OverlapShape,
    TestMove,
}

/// A collider the host can answer questions about.
#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    pub entity: u64,
    /// `box`, `circle`, `polygon` or `edge`.
    pub kind: String,
    pub center: [f64; 2],
    /// Turn in radians, used by boxes.
    pub angle: f64,
    /// Box half-width and half-height. A circle uses `half[0]` as its radius.
    pub half: [f64; 2],
    /// Polygon or edge points in world units. Polygons are treated as convex.
    pub points: Vec<[f64; 2]>,
    /// Layer bits. Bit 0 is layer 1.
    pub layer: u32,
    /// Overlaps are reported, but sweeps and test moves skip it.
    pub sensor: bool,
}

/// Every hit of a ray, nearest first.
#[derive(Clone, Debug, PartialEq)]
pub struct RayList {
    pub entities: Vec<Value>,
    pub points: Vec<Value>,
    pub normals: Vec<Value>,
    pub distances: Vec<Value>,
}

/// The first hit of a sweep or a test move. `hit` is false when nothing is in the way.
#[derive(Clone, Debug, PartialEq)]
pub struct Cast {
    pub entity: u64,
    pub distance: f64,
    pub normal: V3,
    pub point: V3,
    pub hit: bool,
}

#[derive(Clone, Copy)]
struct Hit {
    entity: u64,
    distance: f64,
    point: [f64; 2],
    normal: [f64; 2],
}

fn v3(p: [f64; 2]) -> V3 {
    V3 { x: p[0], y: p[1], z: 0.0 }
}

fn allows(layer: u32, mask: i64) -> bool {
    if mask <= 0 {
        return true;
    }
    let bits = if mask > u32::MAX as i64 { u32::MAX } else { mask as u32 };
    layer & bits != 0
}

fn norm(v: [f64; 2]) -> [f64; 2] {
    let len = (v[0] * v[0] + v[1] * v[1]).sqrt();
    if len < 1e-12 {
        [0.0, 1.0]
    } else {
        [v[0] / len, v[1] / len]
    }
}

fn dot(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn add(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] + b[0], a[1] + b[1]]
}

fn mul(a: [f64; 2], s: f64) -> [f64; 2] {
    [a[0] * s, a[1] * s]
}

fn rot(p: [f64; 2], angle: f64) -> [f64; 2] {
    let (s, c) = angle.sin_cos();
    [c * p[0] - s * p[1], s * p[0] + c * p[1]]
}

fn unrot(p: [f64; 2], angle: f64) -> [f64; 2] {
    let (s, c) = angle.sin_cos();
    [c * p[0] + s * p[1], -s * p[0] + c * p[1]]
}

fn box_corners(center: [f64; 2], angle: f64, half: [f64; 2]) -> [[f64; 2]; 4] {
    let local =
        [[-half[0], -half[1]], [half[0], -half[1]], [half[0], half[1]], [-half[0], half[1]]];
    local.map(|p| add(center, rot(p, angle)))
}

fn hull(points: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let mut p = points.to_vec();
    p.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    p.dedup_by(|a, b| (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9);
    if p.len() < 3 {
        return p;
    }
    let cross = |o: [f64; 2], a: [f64; 2], b: [f64; 2]| {
        (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    };
    let mut lower = Vec::new();
    for &pt in &p {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], pt) <= 0.0 {
            lower.pop();
        }
        lower.push(pt);
    }
    let mut upper = Vec::new();
    for &pt in p.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], pt) <= 0.0 {
            upper.pop();
        }
        upper.push(pt);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

fn convex_of(shape: &Shape) -> Vec<[f64; 2]> {
    match shape.kind.as_str() {
        "circle" => Vec::new(),
        "polygon" => hull(&shape.points),
        "edge" => shape.points.clone(),
        _ => box_corners(shape.center, shape.angle, shape.half).to_vec(),
    }
}

fn ray_circle(origin: [f64; 2], dir: [f64; 2], center: [f64; 2], radius: f64) -> Option<Hit> {
    let oc = sub(origin, center);
    let b = dot(oc, dir);
    let c = dot(oc, oc) - radius * radius;
    let disc = b * b - c;
    if disc < 0.0 {
        return None;
    }
    let s = disc.sqrt();
    let t = if -b - s >= 0.0 {
        -b - s
    } else if -b + s >= 0.0 {
        0.0
    } else {
        return None;
    };
    let point = add(origin, mul(dir, t));
    let normal =
        if t == 0.0 && c < 0.0 { norm(sub(center, origin)) } else { norm(sub(point, center)) };
    Some(Hit { entity: 0, distance: t, point, normal })
}

fn ray_aabb_local(origin: [f64; 2], dir: [f64; 2], half: [f64; 2]) -> Option<(f64, [f64; 2])> {
    let mut t0 = 0.0_f64;
    let mut t1 = f64::INFINITY;
    let mut n = [0.0, 1.0];
    for i in 0..2 {
        let h = half[i].abs();
        if dir[i].abs() < 1e-12 {
            if origin[i] < -h || origin[i] > h {
                return None;
            }
            continue;
        }
        let inv = 1.0 / dir[i];
        let (mut a, mut b) = ((-h - origin[i]) * inv, (h - origin[i]) * inv);
        let mut sign = if dir[i] > 0.0 { -1.0 } else { 1.0 };
        if a > b {
            std::mem::swap(&mut a, &mut b);
            sign = -sign;
        }
        if a > t0 {
            t0 = a;
            n = [0.0, 0.0];
            n[i] = sign;
        }
        t1 = t1.min(b);
        if t0 > t1 {
            return None;
        }
    }
    if t1 < 0.0 {
        return None;
    }
    if t0 >= 0.0 {
        return Some((t0, n));
    }
    let pen = [half[0].abs() - origin[0].abs(), half[1].abs() - origin[1].abs()];
    let n = if pen[0] <= pen[1] { [origin[0].signum(), 0.0] } else { [0.0, origin[1].signum()] };
    Some((0.0, n))
}

fn ray_box(origin: [f64; 2], dir: [f64; 2], shape: &Shape) -> Option<Hit> {
    let local_o = unrot(sub(origin, shape.center), shape.angle);
    let local_d = unrot(dir, shape.angle);
    let (t, local_n) = ray_aabb_local(local_o, local_d, shape.half)?;
    let point = add(origin, mul(dir, t));
    Some(Hit { entity: 0, distance: t, point, normal: norm(rot(local_n, shape.angle)) })
}

fn ray_segment(origin: [f64; 2], dir: [f64; 2], a: [f64; 2], b: [f64; 2]) -> Option<Hit> {
    let edge = sub(b, a);
    let denom = dir[0] * edge[1] - dir[1] * edge[0];
    if denom.abs() < 1e-12 {
        return None;
    }
    let ao = sub(a, origin);
    let t = (ao[0] * edge[1] - ao[1] * edge[0]) / denom;
    let u = (ao[0] * dir[1] - ao[1] * dir[0]) / denom;
    if t < -1e-8 || !(0.0..=1.0).contains(&u) {
        return None;
    }
    let t = t.max(0.0);
    let mut n = [edge[1], -edge[0]];
    if dot(n, dir) > 0.0 {
        n = [-n[0], -n[1]];
    }
    Some(Hit { entity: 0, distance: t, point: add(origin, mul(dir, t)), normal: norm(n) })
}

fn point_in_poly(p: [f64; 2], poly: &[[f64; 2]]) -> bool {
    if poly.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (a, b) = (poly[j], poly[i]);
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let x = (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1] + 1e-18) + a[0];
            if p[0] < x {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

fn point_in_shape(p: [f64; 2], shape: &Shape) -> bool {
    match shape.kind.as_str() {
        "circle" => {
            dot(sub(p, shape.center), sub(p, shape.center)) <= shape.half[0] * shape.half[0] + 1e-9
        }
        "edge" => false,
        "polygon" => point_in_poly(p, &hull(&shape.points)),
        _ => {
            let local = unrot(sub(p, shape.center), shape.angle);
            local[0].abs() <= shape.half[0].abs() + 1e-9
                && local[1].abs() <= shape.half[1].abs() + 1e-9
        }
    }
}

fn ray_shape(origin: [f64; 2], dir: [f64; 2], shape: &Shape) -> Option<Hit> {
    let mut hit = match shape.kind.as_str() {
        "circle" => ray_circle(origin, dir, shape.center, shape.half[0].abs()),
        "polygon" | "edge" => {
            let pts =
                if shape.kind == "polygon" { hull(&shape.points) } else { shape.points.clone() };
            let mut best: Option<Hit> = None;
            let n = pts.len();
            let edges = if shape.kind == "polygon" && n >= 3 { n } else { n.saturating_sub(1) };
            for i in 0..edges {
                let b = pts[(i + 1) % n];
                if let Some(h) = ray_segment(origin, dir, pts[i], b) {
                    if best.is_none_or(|k| h.distance < k.distance) {
                        best = Some(h);
                    }
                }
            }
            if shape.kind == "polygon" && point_in_poly(origin, &pts) {
                best = Some(Hit { entity: 0, distance: 0.0, point: origin, normal: [0.0, 1.0] });
            }
            best
        }
        _ => ray_box(origin, dir, shape),
    }?;
    if shape.kind != "polygon"
        && shape.kind != "edge"
        && point_in_shape(origin, shape)
        && hit.distance > 0.0
    {
        hit.distance = 0.0;
        hit.point = origin;
    }
    hit.entity = shape.entity;
    Some(hit)
}

fn closest_on_segment(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    let ab = sub(b, a);
    let len2 = dot(ab, ab);
    if len2 < 1e-18 {
        return a;
    }
    let t = dot(sub(p, a), ab) / len2;
    add(a, mul(ab, t.clamp(0.0, 1.0)))
}

fn project(poly: &[[f64; 2]], axis: [f64; 2]) -> (f64, f64) {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for p in poly {
        let d = dot(*p, axis);
        lo = lo.min(d);
        hi = hi.max(d);
    }
    (lo, hi)
}

fn axes_of(poly: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let mut out = Vec::new();
    if poly.len() < 2 {
        return out;
    }
    for i in 0..poly.len() {
        let e = sub(poly[(i + 1) % poly.len()], poly[i]);
        let n = norm([e[1], -e[0]]);
        if n[0].abs() + n[1].abs() > 0.0 {
            out.push(n);
        }
    }
    out
}

fn sat_overlap(a: &[[f64; 2]], b: &[[f64; 2]]) -> bool {
    if a.len() < 2 || b.len() < 2 {
        return false;
    }
    for axis in axes_of(a).into_iter().chain(axes_of(b)) {
        let (a0, a1) = project(a, axis);
        let (b0, b1) = project(b, axis);
        if a1 < b0 - 1e-8 || b1 < a0 - 1e-8 {
            return false;
        }
    }
    true
}

fn sweep_convex(a: &[[f64; 2]], b: &[[f64; 2]], delta: [f64; 2]) -> Option<(f64, [f64; 2])> {
    if a.len() < 2 || b.len() < 2 {
        return None;
    }
    let mut enter = f64::NEG_INFINITY;
    let mut exit = f64::INFINITY;
    let mut normal = [0.0, 1.0];
    for axis in axes_of(a).into_iter().chain(axes_of(b)) {
        let (a0, a1) = project(a, axis);
        let (b0, b1) = project(b, axis);
        let v = dot(delta, axis);
        let (t_in, t_out) = if v.abs() < 1e-12 {
            if a1 < b0 - 1e-9 || b1 < a0 - 1e-9 {
                return None;
            }
            (f64::NEG_INFINITY, f64::INFINITY)
        } else {
            let (mut i, mut o) = ((b0 - a1) / v, (b1 - a0) / v);
            if i > o {
                std::mem::swap(&mut i, &mut o);
            }
            (i, o)
        };
        if t_in > enter {
            enter = t_in;
            let sign = if v >= 0.0 { -1.0 } else { 1.0 };
            normal = mul(axis, sign);
        }
        exit = exit.min(t_out);
        if enter > exit {
            return None;
        }
    }
    if exit < 0.0 || enter > 1.0 {
        return None;
    }
    let t = enter.max(0.0);
    Some((t, norm(normal)))
}

fn overlap_circle_shape(center: [f64; 2], radius: f64, shape: &Shape) -> bool {
    match shape.kind.as_str() {
        "circle" => {
            let reach = radius + shape.half[0].abs();
            dot(sub(center, shape.center), sub(center, shape.center)) <= reach * reach + 1e-9
        }
        "edge" => shape.points.windows(2).any(|w| {
            let q = closest_on_segment(center, w[0], w[1]);
            dot(sub(center, q), sub(center, q)) <= radius * radius + 1e-9
        }),
        "polygon" => {
            let poly = hull(&shape.points);
            point_in_poly(center, &poly) || edge_near(&poly, center, radius, true)
        }
        _ => {
            let local = unrot(sub(center, shape.center), shape.angle);
            let clamped = [
                local[0].clamp(-shape.half[0].abs(), shape.half[0].abs()),
                local[1].clamp(-shape.half[1].abs(), shape.half[1].abs()),
            ];
            dot(sub(local, clamped), sub(local, clamped)) <= radius * radius + 1e-9
        }
    }
}

fn edge_near(poly: &[[f64; 2]], center: [f64; 2], radius: f64, closed: bool) -> bool {
    if poly.is_empty() {
        return false;
    }
    let n = poly.len();
    let edges = if closed && n >= 2 { n } else { n.saturating_sub(1) };
    (0..edges).any(|i| {
        let q = closest_on_segment(center, poly[i], poly[(i + 1) % n]);
        dot(sub(center, q), sub(center, q)) <= radius * radius + 1e-9
    })
}

fn overlap_box_shape(center: [f64; 2], half: [f64; 2], shape: &Shape) -> bool {
    let mine = box_corners(center, 0.0, half).to_vec();
    match shape.kind.as_str() {
        "circle" => overlap_circle_shape(
            shape.center,
            shape.half[0].abs(),
            &Shape {
                entity: 0,
                kind: "box".into(),
                center,
                angle: 0.0,
                half,
                points: Vec::new(),
                layer: 0,
                sensor: false,
            },
        ),
        "edge" => shape.points.windows(2).any(|w| segment_hits_convex(w[0], w[1], &mine)),
        "polygon" => sat_overlap(&mine, &hull(&shape.points)),
        _ => sat_overlap(&mine, &convex_of(shape)),
    }
}

fn segment_hits_convex(a: [f64; 2], b: [f64; 2], poly: &[[f64; 2]]) -> bool {
    if point_in_poly(a, poly) || point_in_poly(b, poly) {
        return true;
    }
    let n = poly.len();
    (0..n).any(|i| segments_cross(a, b, poly[i], poly[(i + 1) % n]))
}

fn segments_cross(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    let side = |p: [f64; 2], q: [f64; 2], r: [f64; 2]| {
        (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0])
    };
    let ab_c = side(a, b, c);
    let ab_d = side(a, b, d);
    let cd_a = side(c, d, a);
    let cd_b = side(c, d, b);
    ab_c * ab_d <= 0.0 && cd_a * cd_b <= 0.0
}

fn cast_disk_aabb(
    center: [f64; 2],
    dir: [f64; 2],
    radius: f64,
    max: f64,
    box_center: [f64; 2],
    half: [f64; 2],
) -> Option<Hit> {
    let local = sub(center, box_center);
    let h = [half[0].abs(), half[1].abs()];
    if overlap_circle_shape(
        center,
        radius,
        &Shape {
            entity: 0,
            kind: "box".into(),
            center: box_center,
            angle: 0.0,
            half,
            points: Vec::new(),
            layer: 0,
            sensor: false,
        },
    ) {
        let clamped = [local[0].clamp(-h[0], h[0]), local[1].clamp(-h[1], h[1])];
        let normal = if (local[0] - clamped[0]).abs() + (local[1] - clamped[1]).abs() < 1e-8 {
            let pen = [h[0] - local[0].abs(), h[1] - local[1].abs()];
            if pen[0] <= pen[1] {
                [local[0].signum(), 0.0]
            } else {
                [0.0, local[1].signum()]
            }
        } else {
            norm(sub(local, clamped))
        };
        return Some(Hit { entity: 0, distance: 0.0, point: center, normal });
    }
    let mut best: Option<Hit> = None;
    let consider = |hit: Hit, best: &mut Option<Hit>| {
        if hit.distance <= max + 1e-8 && best.is_none_or(|b| hit.distance < b.distance) {
            *best = Some(hit);
        }
    };
    let faces = [
        ([h[0] + radius, 0.0], [1.0, 0.0], 1, true),
        ([-h[0] - radius, 0.0], [-1.0, 0.0], 1, true),
        ([0.0, h[1] + radius], [0.0, 1.0], 0, false),
        ([0.0, -h[1] - radius], [0.0, -1.0], 0, false),
    ];
    for (plane, normal, axis, vertical) in faces {
        let denom = dot(dir, normal);
        if denom >= -1e-12 {
            continue;
        }
        let t = dot(sub(plane, local), normal) / denom;
        if t < -1e-8 {
            continue;
        }
        let at = add(local, mul(dir, t.max(0.0)));
        let along = if vertical { at[1] } else { at[0] };
        let limit = if axis == 1 { h[1] } else { h[0] };
        if along.abs() <= limit + 1e-6 {
            consider(
                Hit {
                    entity: 0,
                    distance: t.max(0.0),
                    point: add(center, mul(dir, t.max(0.0))),
                    normal,
                },
                &mut best,
            );
        }
    }
    let corners = [[h[0], h[1]], [h[0], -h[1]], [-h[0], h[1]], [-h[0], -h[1]]];
    for corner in corners {
        if let Some(hit) = ray_circle(local, dir, corner, radius) {
            consider(
                Hit {
                    entity: 0,
                    distance: hit.distance,
                    point: add(center, mul(dir, hit.distance)),
                    normal: hit.normal,
                },
                &mut best,
            );
        }
    }
    best
}

fn cast_disk(center: [f64; 2], dir: [f64; 2], radius: f64, max: f64, shape: &Shape) -> Option<Hit> {
    let mut hit = match shape.kind.as_str() {
        "circle" => {
            ray_circle(center, dir, shape.center, radius + shape.half[0].abs()).map(|mut h| {
                if h.distance == 0.0 {
                    h.normal = norm(sub(center, shape.center));
                }
                h.point = add(center, mul(dir, h.distance));
                h
            })
        }
        "edge" => {
            let mut best: Option<Hit> = None;
            for w in shape.points.windows(2) {
                if let Some(h) = cast_disk_segment(center, dir, radius, w[0], w[1]) {
                    if best.is_none_or(|b| h.distance < b.distance) {
                        best = Some(h);
                    }
                }
            }
            best
        }
        "polygon" => {
            let poly = hull(&shape.points);
            let mut best: Option<Hit> = None;
            if point_in_poly(center, &poly) || edge_near(&poly, center, radius, true) {
                best = Some(Hit { entity: 0, distance: 0.0, point: center, normal: [0.0, 1.0] });
            }
            let n = poly.len();
            for i in 0..n {
                if let Some(h) = cast_disk_segment(center, dir, radius, poly[i], poly[(i + 1) % n])
                {
                    if best.is_none_or(|b| h.distance < b.distance) {
                        best = Some(h);
                    }
                }
            }
            best
        }
        _ => {
            let local_c = unrot(sub(center, shape.center), shape.angle);
            let local_d = unrot(dir, shape.angle);
            cast_disk_aabb(local_c, local_d, radius, max, [0.0, 0.0], shape.half).map(|h| Hit {
                entity: 0,
                distance: h.distance,
                point: add(center, mul(dir, h.distance)),
                normal: rot(h.normal, shape.angle),
            })
        }
    }?;
    if hit.distance > max + 1e-6 {
        return None;
    }
    hit.entity = shape.entity;
    hit.normal = norm(hit.normal);
    Some(hit)
}

fn cast_disk_segment(
    center: [f64; 2],
    dir: [f64; 2],
    radius: f64,
    a: [f64; 2],
    b: [f64; 2],
) -> Option<Hit> {
    let mut best: Option<Hit> = None;
    let keep = |h: Hit, best: &mut Option<Hit>| {
        if best.is_none_or(|k| h.distance < k.distance) {
            *best = Some(h);
        }
    };
    if let Some(h) = ray_circle(center, dir, a, radius) {
        keep(h, &mut best);
    }
    if let Some(h) = ray_circle(center, dir, b, radius) {
        keep(h, &mut best);
    }
    let edge = sub(b, a);
    let n = norm([edge[1], -edge[0]]);
    for sign in [1.0, -1.0] {
        let offset = mul(n, radius * sign);
        let a2 = add(a, offset);
        let b2 = add(b, offset);
        if let Some(h) = ray_segment(center, dir, a2, b2) {
            keep(Hit { normal: mul(n, sign), ..h }, &mut best);
        }
    }
    best
}

fn cast_box(
    center: [f64; 2],
    half: [f64; 2],
    dir: [f64; 2],
    max: f64,
    shape: &Shape,
) -> Option<Hit> {
    let mine = box_corners(center, 0.0, half);
    let delta = mul(dir, max);
    let poly = match shape.kind.as_str() {
        "circle" => {
            return cast_disk(
                shape.center,
                mul(dir, -1.0),
                shape.half[0].abs(),
                max,
                &Shape {
                    entity: shape.entity,
                    kind: "box".into(),
                    center,
                    angle: 0.0,
                    half,
                    points: Vec::new(),
                    layer: shape.layer,
                    sensor: false,
                },
            )
            .map(|h| Hit {
                entity: shape.entity,
                distance: h.distance,
                point: add(center, mul(dir, h.distance)),
                normal: mul(h.normal, -1.0),
            });
        }
        "edge" => shape.points.clone(),
        "polygon" => hull(&shape.points),
        _ => convex_of(shape),
    };
    if poly.len() < 2 {
        return None;
    }
    let (t, normal) = sweep_convex(&mine, &poly, delta)?;
    let distance = t * max;
    Some(Hit { entity: shape.entity, distance, point: add(center, mul(dir, distance)), normal })
}

fn shapes(host: &mut dyn Host, mask: i64, solid_only: bool) -> Vec<Shape> {
    host.query_shapes()
        .into_iter()
        .filter(|s| s.entity != NO_ENTITY && allows(s.layer, mask) && !(solid_only && s.sensor))
        .collect()
}

fn unit_dir(dir: V3) -> Option<[f64; 2]> {
    let v = [dir.x, dir.y];
    let len = (v[0] * v[0] + v[1] * v[1]).sqrt();
    if len < 1e-9 {
        None
    } else {
        Some([v[0] / len, v[1] / len])
    }
}

/// Every collider a ray meets, nearest first.
pub fn raycast_all(host: &mut dyn Host, from: V3, dir: V3, max: f64, mask: i64) -> RayList {
    let mut hits = Vec::new();
    if let Some(d) = unit_dir(dir) {
        let origin = [from.x, from.y];
        for shape in shapes(host, mask, false) {
            if let Some(hit) = ray_shape(origin, d, &shape) {
                if hit.distance <= max + 1e-6 {
                    hits.push(hit);
                }
            }
        }
    }
    hits.sort_by(|a, b| a.distance.total_cmp(&b.distance).then(a.entity.cmp(&b.entity)));
    let pack = |f: fn(&Hit) -> Value| hits.iter().map(f).collect();
    RayList {
        entities: pack(|h| Value::Entity(h.entity)),
        points: pack(|h| Value::Vec3(v3(h.point))),
        normals: pack(|h| Value::Vec3(v3(h.normal))),
        distances: pack(|h| Value::Float(h.distance)),
    }
}

fn miss(from: V3, max: f64) -> Cast {
    Cast {
        entity: NO_ENTITY,
        distance: max,
        normal: V3 { x: 0.0, y: 0.0, z: 0.0 },
        point: from,
        hit: false,
    }
}

fn cast_of(hit: Hit) -> Cast {
    Cast {
        entity: hit.entity,
        distance: hit.distance,
        normal: v3(hit.normal),
        point: v3(hit.point),
        hit: true,
    }
}

/// First hit of a circle or a box swept along `dir` for `max` units.
pub fn shape_cast(
    host: &mut dyn Host,
    shape: &str,
    size: V2,
    from: V3,
    dir: V3,
    max: f64,
    mask: i64,
) -> Cast {
    let Some(d) = unit_dir(dir) else {
        return miss(from, max);
    };
    let origin = [from.x, from.y];
    let mut best: Option<Hit> = None;
    for target in shapes(host, mask, true) {
        let hit = if shape == "circle" {
            cast_disk(origin, d, (size.x.abs() * 0.5).max(0.0), max, &target)
        } else if shape == "box" {
            cast_box(
                origin,
                [(size.x.abs() * 0.5).max(0.0), (size.y.abs() * 0.5).max(0.0)],
                d,
                max,
                &target,
            )
        } else {
            None
        };
        if let Some(hit) = hit {
            if best.is_none_or(|b| {
                hit.distance < b.distance || (hit.distance == b.distance && hit.entity < b.entity)
            }) {
                best = Some(hit);
            }
        }
    }
    best.map(cast_of).unwrap_or_else(|| miss(from, max))
}

fn id_list(ids: Vec<u64>) -> Value {
    let mut ids = ids;
    ids.sort_unstable();
    ids.dedup();
    Value::List(Elem::Entity, ids.into_iter().map(Value::Entity).collect())
}

/// Entities whose colliders contain `at`.
pub fn point_query(host: &mut dyn Host, at: V3, mask: i64) -> Value {
    let p = [at.x, at.y];
    id_list(
        shapes(host, mask, false)
            .into_iter()
            .filter(|s| point_in_shape(p, s))
            .map(|s| s.entity)
            .collect(),
    )
}

/// Entities overlapping an axis-aligned box. `size` is the full width and height.
pub fn overlap_box(host: &mut dyn Host, center: V3, size: V2, mask: i64) -> Value {
    let c = [center.x, center.y];
    let half = [size.x.abs() * 0.5, size.y.abs() * 0.5];
    id_list(
        shapes(host, mask, false)
            .into_iter()
            .filter(|s| overlap_box_shape(c, half, s))
            .map(|s| s.entity)
            .collect(),
    )
}

/// Entities overlapping a circle.
pub fn overlap_circle(host: &mut dyn Host, center: V3, radius: f64, mask: i64) -> Value {
    let c = [center.x, center.y];
    id_list(
        shapes(host, mask, false)
            .into_iter()
            .filter(|s| overlap_circle_shape(c, radius.abs(), s))
            .map(|s| s.entity)
            .collect(),
    )
}

fn place_points(center: [f64; 2], angle: f64, local: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let (s, c) = angle.sin_cos();
    local
        .iter()
        .map(|p| [center[0] + c * p[0] - s * p[1], center[1] + s * p[0] + c * p[1]])
        .collect()
}

fn overlap_convex(mine: &[[f64; 2]], shape: &Shape) -> bool {
    match shape.kind.as_str() {
        "circle" => {
            point_in_poly(shape.center, mine)
                || edge_near(mine, shape.center, shape.half[0].abs(), true)
        }
        "edge" => shape.points.windows(2).any(|w| segment_hits_convex(w[0], w[1], mine)),
        "polygon" => sat_overlap(mine, &hull(&shape.points)),
        _ => sat_overlap(mine, &convex_of(shape)),
    }
}

fn capsule_overlaps(center: [f64; 2], angle: f64, radius: f64, half_h: f64, shape: &Shape) -> bool {
    let radius = radius.abs();
    let inner = (half_h.abs() - radius).max(0.0);
    let (s, c) = angle.sin_cos();
    let up = [-s, c];
    let a = add(center, mul(up, -inner));
    let b = add(center, mul(up, inner));
    if overlap_circle_shape(a, radius, shape) || overlap_circle_shape(b, radius, shape) {
        return true;
    }
    if inner > 1e-9 {
        let mine = box_corners(center, angle, [radius, inner]).to_vec();
        return overlap_convex(&mine, shape);
    }
    false
}

fn overlaps_query(
    kind: &str,
    size: [f64; 2],
    points: &[[f64; 2]],
    center: [f64; 2],
    angle: f64,
    shape: &Shape,
) -> bool {
    match kind {
        "circle" => overlap_circle_shape(center, size[0].abs() * 0.5, shape),
        "box" => {
            let half = [size[0].abs() * 0.5, size[1].abs() * 0.5];
            overlap_convex(&box_corners(center, angle, half), shape)
        }
        "capsule" => {
            capsule_overlaps(center, angle, size[0].abs() * 0.5, size[1].abs() * 0.5, shape)
        }
        "polygon" => {
            if points.len() < 3 {
                return false;
            }
            overlap_convex(&place_points(center, angle, points), shape)
        }
        _ => false,
    }
}

fn vec2_points(values: &[Value]) -> Vec<[f64; 2]> {
    values
        .iter()
        .map(|value| {
            let point = value.vec2();
            [point.x, point.y]
        })
        .collect()
}

/// Every collider overlapping a box, circle, capsule or polygon. Lowest id first.
/// An unknown shape, or a polygon with fewer than three points, matches nothing.
pub fn overlap_shape(
    host: &mut dyn Host,
    shape: &str,
    size: V2,
    points: &[Value],
    center: V3,
    angle: f64,
    mask: i64,
) -> Value {
    let local = vec2_points(points);
    let at = [center.x, center.y];
    let size = [size.x, size.y];
    id_list(
        shapes(host, mask, false)
            .into_iter()
            .filter(|target| overlaps_query(shape, size, &local, at, angle, target))
            .map(|target| target.entity)
            .collect(),
    )
}

/// Whether `entity` would hit something if it moved by `delta`.
pub fn test_move(host: &mut dyn Host, entity: u64, delta: V3, mask: i64) -> Cast {
    let all = shapes(host, 0, false);
    let Some(mine) = all.iter().find(|s| s.entity == entity) else {
        return miss(V3 { x: 0.0, y: 0.0, z: 0.0 }, 0.0);
    };
    let mine = mine.clone();
    let delta_v = [delta.x, delta.y];
    let len = (delta_v[0] * delta_v[0] + delta_v[1] * delta_v[1]).sqrt();
    if len < 1e-9 {
        return miss(v3(mine.center), 0.0);
    }
    let dir = [delta_v[0] / len, delta_v[1] / len];
    let mut best: Option<Hit> = None;
    for other in
        all.into_iter().filter(|s| s.entity != entity && !s.sensor && allows(s.layer, mask))
    {
        let hit = match mine.kind.as_str() {
            "circle" => cast_disk(mine.center, dir, mine.half[0].abs(), len, &other),
            "polygon" => {
                let poly = hull(&mine.points);
                let (t, normal) = match sweep_convex(&poly, &convex_of(&other), delta_v) {
                    Some(v) => v,
                    None => continue,
                };
                Some(Hit {
                    entity: other.entity,
                    distance: t * len,
                    point: add(mine.center, mul(delta_v, t)),
                    normal,
                })
            }
            "edge" => None,
            _ => cast_box(mine.center, [mine.half[0].abs(), mine.half[1].abs()], dir, len, &other),
        };
        if let Some(hit) = hit {
            if best.is_none_or(|b| hit.distance < b.distance) {
                best = Some(hit);
            }
        }
    }
    best.map(cast_of).unwrap_or_else(|| miss(v3(add(mine.center, delta_v)), len))
}

/// The value of output `pin` for a physics query node.
pub fn eval(kind: Kind, pin: u32, host: &mut dyn Host, args: &[Value]) -> Value {
    match kind {
        Kind::RaycastAll => {
            let list =
                raycast_all(host, args[0].vec3(), args[1].vec3(), args[2].float(), args[3].int());
            match pin {
                0 => Value::List(Elem::Entity, list.entities),
                1 => Value::List(Elem::Vec3, list.points),
                2 => Value::List(Elem::Vec3, list.normals),
                _ => Value::List(Elem::Float, list.distances),
            }
        }
        Kind::ShapeCast => {
            let hit = shape_cast(
                host,
                &args[0].str(),
                args[1].vec2(),
                args[2].vec3(),
                args[3].vec3(),
                args[4].float(),
                args[5].int(),
            );
            match pin {
                0 => Value::Entity(hit.entity),
                1 => Value::Float(hit.distance),
                2 => Value::Vec3(hit.normal),
                3 => Value::Vec3(hit.point),
                _ => Value::Bool(hit.hit),
            }
        }
        Kind::Point => point_query(host, args[0].vec3(), args[1].int()),
        Kind::OverlapBox => overlap_box(host, args[0].vec3(), args[1].vec2(), args[2].int()),
        Kind::OverlapCircle => overlap_circle(host, args[0].vec3(), args[1].float(), args[2].int()),
        Kind::OverlapShape => {
            let points = args[2].list();
            overlap_shape(
                host,
                &args[0].str(),
                args[1].vec2(),
                &points,
                args[3].vec3(),
                args[4].float(),
                args[5].int(),
            )
        }
        Kind::TestMove => {
            let hit = test_move(host, args[0].entity(), args[1].vec3(), args[2].int());
            match pin {
                0 => Value::Bool(hit.hit),
                1 => Value::Entity(hit.entity),
                2 => Value::Float(hit.distance),
                3 => Value::Vec3(hit.normal),
                _ => Value::Vec3(hit.point),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rt::MapHost;

    fn box_at(entity: u64, x: f64, layer: u32) -> Shape {
        Shape {
            entity,
            kind: "box".into(),
            center: [x, 0.0],
            angle: 0.0,
            half: [0.5, 0.5],
            points: Vec::new(),
            layer,
            sensor: false,
        }
    }

    fn host() -> MapHost {
        MapHost {
            shapes: vec![box_at(1, 0.0, 1), box_at(3, 1.5, 2), box_at(2, 3.0, 1)],
            ..MapHost::default()
        }
    }

    #[test]
    fn raycast_all_sorts_hits_and_honours_the_mask() {
        let mut h = host();
        let from = V3 { x: -2.0, y: 0.0, z: 0.0 };
        let dir = V3 { x: 1.0, y: 0.0, z: 0.0 };
        let all = raycast_all(&mut h, from, dir, 10.0, 0);
        let ids: Vec<u64> = all.entities.iter().map(|v| v.entity()).collect();
        assert_eq!(ids, vec![1, 3, 2]);
        assert!((all.distances[0].float() - 1.5).abs() < 1e-6);
        let masked = raycast_all(&mut h, from, dir, 10.0, 1);
        let ids: Vec<u64> = masked.entities.iter().map(|v| v.entity()).collect();
        assert_eq!(ids, vec![1, 2]);
    }

    #[test]
    fn sweeps_points_and_overlaps_agree_on_a_known_layout() {
        let mut h = host();
        let cast = shape_cast(
            &mut h,
            "circle",
            V2 { x: 0.5, y: 0.5 },
            V3 { x: -2.0, y: 0.0, z: 0.0 },
            V3 { x: 1.0, y: 0.0, z: 0.0 },
            10.0,
            0,
        );
        assert!(cast.hit);
        assert_eq!(cast.entity, 1);
        assert!((cast.distance - 1.25).abs() < 1e-4, "{}", cast.distance);
        let inside = point_query(&mut h, V3 { x: 0.0, y: 0.0, z: 0.0 }, 0);
        assert_eq!(inside.list().len(), 1);
        assert_eq!(inside.list()[0].entity(), 1);
        let boxed = overlap_box(&mut h, V3 { x: 0.0, y: 0.0, z: 0.0 }, V2 { x: 4.5, y: 4.0 }, 0);
        let ids: Vec<u64> = boxed.list().iter().map(|v| v.entity()).collect();
        assert_eq!(ids, vec![1, 3]);
        let circled = overlap_circle(&mut h, V3 { x: 3.0, y: 0.0, z: 0.0 }, 0.6, 0);
        assert_eq!(circled.list().iter().map(|v| v.entity()).collect::<Vec<_>>(), vec![2]);
        let shaped = overlap_shape(
            &mut h,
            "box",
            V2 { x: 4.5, y: 4.0 },
            &[],
            V3 { x: 0.0, y: 0.0, z: 0.0 },
            0.0,
            0,
        );
        let ids: Vec<u64> = shaped.list().iter().map(|v| v.entity()).collect();
        assert_eq!(ids, vec![1, 3]);
        let masked = overlap_shape(
            &mut h,
            "capsule",
            V2 { x: 0.5, y: 8.0 },
            &[],
            V3 { x: 1.5, y: 0.0, z: 0.0 },
            0.0,
            1,
        );
        assert!(masked.list().is_empty(), "layer 2 is not in mask 1");
        let poly = [
            Value::Vec2(V2 { x: -0.2, y: -0.2 }),
            Value::Vec2(V2 { x: 0.2, y: -0.2 }),
            Value::Vec2(V2 { x: 0.2, y: 0.2 }),
            Value::Vec2(V2 { x: -0.2, y: 0.2 }),
        ];
        let hit = overlap_shape(
            &mut h,
            "polygon",
            V2 { x: 1.0, y: 1.0 },
            &poly,
            V3 { x: 3.0, y: 0.0, z: 0.0 },
            0.0,
            0,
        );
        assert_eq!(hit.list().iter().map(|v| v.entity()).collect::<Vec<_>>(), vec![2]);
        let none = overlap_shape(
            &mut h,
            "triangle",
            V2 { x: 1.0, y: 1.0 },
            &[],
            V3 { x: 0.0, y: 0.0, z: 0.0 },
            0.0,
            0,
        );
        assert!(none.list().is_empty());
        let short = overlap_shape(
            &mut h,
            "polygon",
            V2 { x: 1.0, y: 1.0 },
            &poly[..2],
            V3 { x: 0.0, y: 0.0, z: 0.0 },
            0.0,
            0,
        );
        assert!(short.list().is_empty());
        let moved = test_move(&mut h, 1, V3 { x: 1.2, y: 0.0, z: 0.0 }, 0);
        assert!(moved.hit);
        assert_eq!(moved.entity, 3);
    }

    #[test]
    fn a_zero_direction_and_an_unknown_shape_hit_nothing() {
        let mut h = host();
        let none = raycast_all(
            &mut h,
            V3 { x: 0.0, y: 0.0, z: 0.0 },
            V3 { x: 0.0, y: 0.0, z: 0.0 },
            5.0,
            0,
        );
        assert!(none.entities.is_empty());
        let bad = shape_cast(
            &mut h,
            "triangle",
            V2 { x: 1.0, y: 1.0 },
            V3 { x: 0.0, y: 0.0, z: 0.0 },
            V3 { x: 1.0, y: 0.0, z: 0.0 },
            4.0,
            0,
        );
        assert!(!bad.hit);
    }
}
