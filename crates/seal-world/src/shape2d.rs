//! Drawn 2D lines and filled shapes, and objects that follow them in play.
//!
//! A shape's points sit around its object in the object's own units; the
//! object's position, scale and turn place them in the scene. A follower
//! names the object whose line it rides along.

use crate::phys2d::{parse_points, points_text};
use crate::{format_num, parse_tint, tint_hex, Entity};

pub const KINDS: &[&str] = &["none", "line", "polygon"];
pub const MODES: &[&str] = &["loop", "once", "back"];
/// Inspector fields, in order: the shape, then following.
pub const KEYS: &[&str] = &[
    "shape2d",
    "shape_points",
    "shape_width",
    "shape_color",
    "shape_closed",
    "follow_path",
    "follow_speed",
    "follow_mode",
    "follow_rotate",
    "follow_start",
];
/// Picture, line-style and end-cap fields. Kept off `KEYS` so Follow Path stays the tail of that list.
pub const EXTRA: &[&str] = &[
    "shape_texture",
    "shape_uv_offset",
    "shape_uv_scale",
    "shape_uv_rotation",
    "shape_width_curve",
    "shape_gradient",
    "shape_cap_begin",
    "shape_cap_end",
];
pub const CAPS: &[&str] = &["none", "square", "round"];

/// Whether an inspector or copy key belongs to a 2D shape or its follower.
pub fn known(key: &str) -> bool {
    KEYS.contains(&key) || EXTRA.contains(&key)
}

/// Text written when the field is untouched. A save skips an extra field that still reads like this.
pub fn default_text(key: &str) -> &'static str {
    match key {
        "shape_uv_offset" => "0, 0",
        "shape_uv_scale" => "1, 1",
        "shape_uv_rotation" => "0",
        "shape_cap_begin" | "shape_cap_end" => "none",
        _ => "",
    }
}

fn yes(on: bool) -> String {
    if on { "yes" } else { "no" }.into()
}

/// The shape's rows: just the kind until there is one.
pub fn shape_fields(e: &Entity) -> Vec<(String, String)> {
    let s = &e.shape2d;
    let kind = if s.kind.is_empty() { "none".to_string() } else { s.kind.clone() };
    let mut out = vec![("shape2d".to_string(), kind)];
    if !s.kind.is_empty() {
        out.push(("shape_points".into(), points_text(&s.points)));
        if s.kind == "line" {
            out.push(("shape_width".into(), format_num(s.width)));
        }
        out.push(("shape_color".into(), tint_hex(&s.color)));
        if s.kind == "line" {
            out.push(("shape_closed".into(), yes(s.closed)));
            out.push(("shape_cap_begin".into(), cap_text(&s.cap_begin)));
            out.push(("shape_cap_end".into(), cap_text(&s.cap_end)));
            out.push(("shape_width_curve".into(), s.width_curve.clone()));
            out.push(("shape_gradient".into(), s.gradient.clone()));
        }
        if s.kind == "polygon" {
            out.push(("shape_texture".into(), s.texture.clone()));
            out.push(("shape_uv_offset".into(), pair_text(s.uv_offset)));
            out.push(("shape_uv_scale".into(), pair_text(s.uv_scale)));
            out.push(("shape_uv_rotation".into(), format_num(s.uv_rotation)));
        }
    }
    out
}

fn cap_text(stored: &str) -> String {
    if stored.is_empty() {
        "none".into()
    } else {
        stored.to_string()
    }
}

fn pair_text(v: [f32; 2]) -> String {
    format!("{}, {}", format_num(v[0]), format_num(v[1]))
}

fn parse_pair(text: &str) -> Option<[f32; 2]> {
    let parts: Vec<&str> = text.split(',').map(str::trim).filter(|p| !p.is_empty()).collect();
    if parts.len() != 2 {
        return None;
    }
    let x = parts[0].parse::<f32>().ok().filter(|n| n.is_finite())?;
    let y = parts[1].parse::<f32>().ok().filter(|n| n.is_finite())?;
    Some([x, y])
}

/// The following rows: just the path until one is named.
pub fn follow_fields(e: &Entity) -> Vec<(String, String)> {
    let f = &e.follow;
    let mut out = vec![("follow_path".to_string(), f.path.clone())];
    if !f.path.is_empty() {
        out.push(("follow_speed".into(), format_num(f.speed)));
        out.push(("follow_mode".into(), f.mode.clone()));
        out.push(("follow_rotate".into(), yes(f.rotate)));
        out.push(("follow_start".into(), format_num(f.start)));
    }
    out
}

pub fn field_text(e: &Entity, key: &str) -> Option<String> {
    let (s, f) = (&e.shape2d, &e.follow);
    Some(match key {
        "shape2d" => {
            if s.kind.is_empty() {
                "none".into()
            } else {
                s.kind.clone()
            }
        }
        "shape_points" => points_text(&s.points),
        "shape_width" => format_num(s.width),
        "shape_color" => tint_hex(&s.color),
        "shape_closed" => yes(s.closed),
        "shape_texture" => s.texture.clone(),
        "shape_uv_offset" => pair_text(s.uv_offset),
        "shape_uv_scale" => pair_text(s.uv_scale),
        "shape_uv_rotation" => format_num(s.uv_rotation),
        "shape_width_curve" => s.width_curve.clone(),
        "shape_gradient" => s.gradient.clone(),
        "shape_cap_begin" => cap_text(&s.cap_begin),
        "shape_cap_end" => cap_text(&s.cap_end),
        "follow_path" => f.path.clone(),
        "follow_speed" => format_num(f.speed),
        "follow_mode" => f.mode.clone(),
        "follow_rotate" => yes(f.rotate),
        "follow_start" => format_num(f.start),
        _ => return None,
    })
}

/// Why a field's text is refused, or `None` when it is fine.
pub fn check(key: &str, value: &str) -> Option<&'static str> {
    let v = value.trim();
    let num =
        |lo: f32, hi: f32| v.parse::<f32>().is_ok_and(|n| n.is_finite() && (lo..=hi).contains(&n));
    let ok = match key {
        "shape2d" => KINDS.contains(&v),
        "shape_points" => parse_points(v).is_some(),
        "shape_width" => num(0.001, 100.0),
        "shape_color" => parse_tint(v).is_some(),
        "shape_closed" | "follow_rotate" => matches!(v, "yes" | "no"),
        "shape_texture" | "shape_width_curve" | "shape_gradient" => true,
        "shape_uv_offset" | "shape_uv_scale" => {
            parse_pair(v).is_some_and(|p| p.iter().all(|n| (-1000.0..=1000.0).contains(n)))
        }
        "shape_uv_rotation" => num(-100000.0, 100000.0),
        "shape_cap_begin" | "shape_cap_end" => CAPS.contains(&v),
        "follow_path" => true,
        "follow_speed" => num(-1000.0, 1000.0),
        "follow_mode" => MODES.contains(&v),
        "follow_start" => num(0.0, 1.0e6),
        _ => false,
    };
    if ok {
        return None;
    }
    Some(match key {
        "shape_points" => "inspector-shape-points-invalid",
        "shape_color" => "inspector-shape-color-invalid",
        _ => "inspector-value-out-of-range",
    })
}

pub fn apply(e: &mut Entity, key: &str, value: &str) -> bool {
    if !known(key) || check(key, value).is_some() {
        return false;
    }
    let v = value.trim();
    let (s, f) = (&mut e.shape2d, &mut e.follow);
    match key {
        "shape2d" => {
            s.kind = if v == "none" { String::new() } else { v.into() };
            // A new shape starts as something visible.
            if !s.kind.is_empty() && s.points.len() < 2 {
                s.points = vec![[-0.5, -0.5], [0.5, -0.5], [0.5, 0.5], [-0.5, 0.5]];
            }
        }
        "shape_points" => s.points = parse_points(v).unwrap_or_default(),
        "shape_width" => s.width = v.parse().unwrap_or(s.width),
        "shape_color" => s.color = parse_tint(v).unwrap_or(s.color),
        "shape_closed" => s.closed = v == "yes",
        "shape_texture" => s.texture = v.replace('\\', "/"),
        "shape_uv_offset" => s.uv_offset = parse_pair(v).unwrap_or(s.uv_offset),
        "shape_uv_scale" => s.uv_scale = parse_pair(v).unwrap_or(s.uv_scale),
        "shape_uv_rotation" => s.uv_rotation = v.parse().unwrap_or(s.uv_rotation),
        "shape_width_curve" => s.width_curve = v.to_string(),
        "shape_gradient" => s.gradient = v.to_string(),
        "shape_cap_begin" => s.cap_begin = if v == "none" { String::new() } else { v.into() },
        "shape_cap_end" => s.cap_end = if v == "none" { String::new() } else { v.into() },
        "follow_path" => f.path = v.into(),
        "follow_speed" => f.speed = v.parse().unwrap_or(f.speed),
        "follow_mode" => f.mode = v.into(),
        "follow_rotate" => f.rotate = v == "yes",
        "follow_start" => f.start = v.parse().unwrap_or(f.start),
        _ => return false,
    }
    true
}

/// The shape's points placed in the scene's x, y plane: scaled, turned and moved with the object.
pub fn world_points(e: &Entity) -> Vec<[f32; 2]> {
    let (sin, cos) = e.rotation[2].to_radians().sin_cos();
    e.shape2d
        .points
        .iter()
        .map(|p| {
            let (x, y) = (p[0] * e.scale[0], p[1] * e.scale[1]);
            [e.translation[0] + x * cos - y * sin, e.translation[1] + x * sin + y * cos]
        })
        .collect()
}

/// A scene point turned back into the object's own units. The reverse of [`world_points`].
pub fn local_point(e: &Entity, world: [f32; 2]) -> [f32; 2] {
    let (dx, dy) = (world[0] - e.translation[0], world[1] - e.translation[1]);
    let (sin, cos) = e.rotation[2].to_radians().sin_cos();
    let x = dx * cos + dy * sin;
    let y = -dx * sin + dy * cos;
    let sx = if e.scale[0].abs() < 1.0e-6 { 1.0 } else { e.scale[0] };
    let sy = if e.scale[1].abs() < 1.0e-6 { 1.0 } else { e.scale[1] };
    [x / sx, y / sy]
}

/// Whether the outline joins back to its start: always for a filled shape.
pub fn is_closed(s: &seal_scene::Shape2d) -> bool {
    s.kind == "polygon" || s.closed
}

/// Corner points of an outline as it is walked, with the start repeated at the end when closed.
fn track(e: &Entity) -> Vec<[f32; 2]> {
    let mut pts = world_points(e);
    if is_closed(&e.shape2d) && pts.len() > 2 {
        pts.push(pts[0]);
    }
    pts
}

fn length(pts: &[[f32; 2]]) -> f32 {
    pts.windows(2).map(|w| ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt()).sum()
}

/// Where along `pts` a distance `d` lands, and the direction there in degrees.
pub fn point_at(pts: &[[f32; 2]], d: f32) -> Option<([f32; 2], f32)> {
    let first = *pts.first()?;
    let mut left = d.max(0.0);
    let mut heading = 0.0;
    for w in pts.windows(2) {
        let (dx, dy) = (w[1][0] - w[0][0], w[1][1] - w[0][1]);
        let seg = (dx * dx + dy * dy).sqrt();
        if seg <= 1e-6 {
            continue;
        }
        heading = dy.atan2(dx).to_degrees();
        if left <= seg {
            let t = left / seg;
            return Some(([w[0][0] + dx * t, w[0][1] + dy * t], heading));
        }
        left -= seg;
    }
    Some((*pts.last().unwrap_or(&first), heading))
}

/// Distance travelled along each follower's path this play.
#[derive(Default)]
pub struct Followers {
    travelled: Vec<(String, f32)>,
    /// Followers that reached the end of a `once` path.
    pub finished: std::collections::BTreeSet<String>,
}

impl Followers {
    pub fn step(&mut self, dt: f32, entities: &mut [Entity]) {
        if dt <= 0.0 {
            return;
        }
        let mut moves = Vec::new();
        for (i, e) in entities.iter().enumerate() {
            let f = &e.follow;
            if f.path.is_empty() || e.hidden {
                continue;
            }
            let Some(path) = entities.iter().find(|o| {
                o.id != e.id && !o.shape2d.kind.is_empty() && (o.name == f.path || o.id == f.path)
            }) else {
                continue;
            };
            let pts = track(path);
            let total = length(&pts);
            if total <= 1e-6 {
                continue;
            }
            let slot = match self.travelled.iter().position(|(id, _)| id == &e.id) {
                Some(s) => s,
                None => {
                    self.travelled.push((e.id.clone(), f.start));
                    self.travelled.len() - 1
                }
            };
            let d = &mut self.travelled[slot].1;
            *d += f.speed * dt;
            // Where on the path the distance travelled puts us, and which way we face.
            let (along, backwards) = match f.mode.as_str() {
                "once" => {
                    if *d >= total {
                        self.finished.insert(e.id.clone());
                    }
                    (d.clamp(0.0, total), false)
                }
                "back" => {
                    let lap = d.rem_euclid(2.0 * total);
                    if lap > total {
                        (2.0 * total - lap, true)
                    } else {
                        (lap, false)
                    }
                }
                _ => (d.rem_euclid(total), false),
            };
            let Some((p, heading)) = point_at(&pts, along) else { continue };
            let heading = if backwards != (f.speed < 0.0) { heading + 180.0 } else { heading };
            moves.push((i, p, f.rotate.then_some(heading)));
        }
        for (i, p, turn) in moves {
            entities[i].translation[0] = p[0];
            entities[i].translation[1] = p[1];
            if let Some(deg) = turn {
                entities[i].rotation[2] = deg;
            }
        }
    }
}

/// Split a simple outline into triangles (ear clipping), as index triples.
pub fn triangulate(pts: &[[f32; 2]]) -> Vec<[usize; 3]> {
    let n = pts.len();
    if n < 3 {
        return Vec::new();
    }
    let area: f32 = (0..n)
        .map(|i| {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum();
    let mut idx: Vec<usize> = if area >= 0.0 { (0..n).collect() } else { (0..n).rev().collect() };
    let cross = |a: [f32; 2], b: [f32; 2], c: [f32; 2]| {
        (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
    };
    let mut out = Vec::new();
    let mut guard = 0;
    while idx.len() > 3 && guard < n * n {
        guard += 1;
        let m = idx.len();
        let mut cut = None;
        for k in 0..m {
            let (ia, ib, ic) = (idx[(k + m - 1) % m], idx[k], idx[(k + 1) % m]);
            let (a, b, c) = (pts[ia], pts[ib], pts[ic]);
            if cross(a, b, c) <= 1e-9 {
                continue;
            }
            let inside = idx.iter().any(|&j| {
                j != ia && j != ib && j != ic && {
                    let p = pts[j];
                    cross(a, b, p) >= 0.0 && cross(b, c, p) >= 0.0 && cross(c, a, p) >= 0.0
                }
            });
            if !inside {
                cut = Some(k);
                out.push([ia, ib, ic]);
                break;
            }
        }
        match cut {
            Some(k) => {
                idx.remove(k);
            }
            None => break,
        }
    }
    if idx.len() == 3 {
        out.push([idx[0], idx[1], idx[2]]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_l_shape_splits_into_four_triangles_covering_its_area() {
        let l = [[0.0, 0.0], [2.0, 0.0], [2.0, 1.0], [1.0, 1.0], [1.0, 2.0], [0.0, 2.0]];
        let tris = triangulate(&l);
        assert_eq!(tris.len(), 4);
        let area: f32 = tris
            .iter()
            .map(|t| {
                let (a, b, c) = (l[t[0]], l[t[1]], l[t[2]]);
                ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs() / 2.0
            })
            .sum();
        assert!((area - 3.0).abs() < 1e-5, "{area}");
    }

    #[test]
    fn point_at_walks_the_corners() {
        let pts = [[0.0, 0.0], [2.0, 0.0], [2.0, 2.0]];
        assert_eq!(point_at(&pts, 1.0), Some(([1.0, 0.0], 0.0)));
        assert_eq!(point_at(&pts, 3.0), Some(([2.0, 1.0], 90.0)));
        assert_eq!(point_at(&pts, 9.0), Some(([2.0, 2.0], 90.0)));
    }
}
