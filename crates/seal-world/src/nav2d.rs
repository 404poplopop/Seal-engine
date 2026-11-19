//! Flat 2D navigation for play: a walkable map of the scene built from its
//! ground tiles and solid 2D bodies, shortest routes across it with the
//! corners cut where the way is clear, and walkers that follow those routes
//! while stepping around each other.
//!
//! The map is a grid on the scene's x, y plane. A cell is blocked when a
//! ground tile or a fixed or kinematic body (other than a one-way platform)
//! covers it, grown by the walker's radius so it keeps clear of walls.

use crate::nav::NavGrid;
use crate::{format_num, Entity};

/// Grid cell size, in scene units.
pub const CELL: f32 = 0.25;
/// Seconds between route updates for a walker after a moving object.
const REPLAN: f32 = 0.25;
/// Inspector fields, in order.
pub const KEYS: &[&str] = &["nav_agent", "nav_target", "nav_speed", "nav_radius", "nav_avoid"];

pub fn fields(n: &seal_scene::NavAgent2d) -> Vec<(String, String)> {
    let mut out = vec![("nav_agent".to_string(), yes(n.on))];
    if n.on {
        out.push(("nav_target".into(), n.target.clone()));
        out.push(("nav_speed".into(), format_num(n.speed)));
        out.push(("nav_radius".into(), format_num(n.radius)));
        out.push(("nav_avoid".into(), yes(n.avoid)));
    }
    out
}

fn yes(on: bool) -> String {
    if on { "yes" } else { "no" }.into()
}

pub fn field_text(n: &seal_scene::NavAgent2d, key: &str) -> Option<String> {
    fields(&seal_scene::NavAgent2d { on: true, ..n.clone() })
        .into_iter()
        .find(|(k, _)| k == key)
        .map(|(k, v)| if k == "nav_agent" { yes(n.on) } else { v })
}

/// Why a field's text is refused, or `None` when it is fine.
pub fn check(key: &str, value: &str) -> Option<&'static str> {
    let v = value.trim();
    let num =
        |lo: f32, hi: f32| v.parse::<f32>().is_ok_and(|n| n.is_finite() && (lo..=hi).contains(&n));
    let ok = match key {
        "nav_agent" | "nav_avoid" => matches!(v, "yes" | "no"),
        "nav_target" => true,
        "nav_speed" => num(0.0, 100.0),
        "nav_radius" => num(0.05, 10.0),
        _ => false,
    };
    (!ok).then_some("inspector-value-out-of-range")
}

pub fn apply(n: &mut seal_scene::NavAgent2d, key: &str, value: &str) -> bool {
    if check(key, value).is_some() {
        return false;
    }
    let v = value.trim();
    match key {
        "nav_agent" => n.on = v == "yes",
        "nav_avoid" => n.avoid = v == "yes",
        "nav_target" => n.target = v.into(),
        "nav_speed" => n.speed = v.parse().unwrap_or(n.speed),
        "nav_radius" => n.radius = v.parse().unwrap_or(n.radius),
        _ => return false,
    }
    true
}

/// A walker's target: a fixed point, or another object followed as it moves.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    Point([f32; 2]),
    Object(String),
}

/// Read a target: `x, y` is a point, anything else names an object.
pub fn parse_target(text: &str) -> Option<Target> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    match crate::phys2d::parse_pair(t) {
        Some(p) => Some(Target::Point(p)),
        None => Some(Target::Object(t.to_string())),
    }
}

/// Half width and height of what an object blocks, or `None` when it does not block.
fn blocker(e: &Entity) -> Option<[f32; 2]> {
    if e.hidden || e.nav2d.on {
        return None;
    }
    let s = [e.scale[0].abs(), e.scale[1].abs()];
    let p = &e.phys2d;
    if matches!(p.body.as_str(), "fixed" | "kinematic") && !p.one_way {
        return Some([p.size[0] * s[0] * 0.5, p.size[1] * s[1] * 0.5]);
    }
    (p.body.is_empty() && matches!(e.tag.as_str(), "ground" | "nav_block"))
        .then_some([s[0] * 0.5, s[1] * 0.5])
}

/// The walkable map for walkers of `radius`, covering every object plus a margin.
pub fn map(entities: &[Entity], radius: f32) -> NavGrid {
    map_with(entities, radius, &[])
}

/// [`map`], also covering `extra` points such as a target that wandered off.
fn map_with(entities: &[Entity], radius: f32, extra: &[[f32; 2]]) -> NavGrid {
    let mut lo = [f32::MAX; 2];
    let mut hi = [f32::MIN; 2];
    let points = entities
        .iter()
        .filter(|e| !e.hidden)
        .map(|e| [e.translation[0], e.translation[1]])
        .chain(extra.iter().copied());
    for p in points {
        lo = [lo[0].min(p[0]), lo[1].min(p[1])];
        hi = [hi[0].max(p[0]), hi[1].max(p[1])];
    }
    if lo[0] > hi[0] {
        lo = [0.0; 2];
        hi = [0.0; 2];
    }
    let margin = 4.0;
    let origin = (lo[0] - margin, lo[1] - margin);
    let w = (((hi[0] - lo[0] + 2.0 * margin) / CELL).ceil() as usize).clamp(1, 1024);
    let h = (((hi[1] - lo[1] + 2.0 * margin) / CELL).ceil() as usize).clamp(1, 1024);
    let mut grid = NavGrid::new(w, h, CELL, origin);
    for e in entities {
        let Some(half) = blocker(e) else { continue };
        let (cx, cy) = (e.translation[0], e.translation[1]);
        let (x0, x1) = (cx - half[0] - radius, cx + half[0] + radius);
        let (y0, y1) = (cy - half[1] - radius, cy + half[1] + radius);
        let gx0 = (((x0 - origin.0) / CELL).floor().max(0.0)) as usize;
        let gy0 = (((y0 - origin.1) / CELL).floor().max(0.0)) as usize;
        let gx1 = (((x1 - origin.0) / CELL).ceil().max(0.0) as usize).min(w);
        let gy1 = (((y1 - origin.1) / CELL).ceil().max(0.0) as usize).min(h);
        for y in gy0..gy1 {
            for x in gx0..gx1 {
                let (px, py) = grid.centre_of((x, y));
                if px > x0 && px < x1 && py > y0 && py < y1 {
                    grid.set_blocked(x, y, true);
                }
            }
        }
    }
    grid
}

/// The walkable cell centre closest to `p`, searching outwards ring by ring.
fn nearest_free(grid: &NavGrid, p: [f32; 2]) -> Option<[f32; 2]> {
    let cx = ((p[0] - grid.origin.0) / grid.cell).floor() as i64;
    let cy = ((p[1] - grid.origin.1) / grid.cell).floor() as i64;
    for r in 1..64i64 {
        let mut best: Option<([f32; 2], f32)> = None;
        for y in cy - r..=cy + r {
            for x in cx - r..=cx + r {
                if (x - cx).abs() != r && (y - cy).abs() != r || !grid.is_walkable(x, y) {
                    continue;
                }
                let (px, py) = grid.centre_of((x as usize, y as usize));
                let d = dist([px, py], p);
                if best.is_none_or(|(_, b)| d < b) {
                    best = Some(([px, py], d));
                }
            }
        }
        if let Some((q, _)) = best {
            return Some(q);
        }
    }
    None
}

fn walkable_at(grid: &NavGrid, p: [f32; 2]) -> bool {
    grid.cell_of((p[0], p[1])).is_some_and(|c| grid.is_walkable(c.0 as i64, c.1 as i64))
}

/// Whether the straight line from `a` to `b` stays on walkable cells.
pub fn clear_line(grid: &NavGrid, a: [f32; 2], b: [f32; 2]) -> bool {
    let d = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
    let n = (d / (grid.cell * 0.4)).ceil().max(1.0) as usize;
    (0..=n).all(|i| {
        let t = i as f32 / n as f32;
        walkable_at(grid, [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t])
    })
}

/// The shortest route from `from` to `to` as corner points, cutting corners
/// wherever the straight way is clear. The first point is the next one to
/// head for; the last is `to` itself.
pub fn route(grid: &NavGrid, from: [f32; 2], to: [f32; 2]) -> Option<Vec<[f32; 2]>> {
    // A target inside something solid: walk up to the nearest free spot next to it.
    let to = if walkable_at(grid, to) { to } else { nearest_free(grid, to)? };
    if clear_line(grid, from, to) {
        return Some(vec![to]);
    }
    let cells = grid.find_path((from[0], from[1]), (to[0], to[1]))?;
    let mut pts: Vec<[f32; 2]> = cells.iter().map(|c| [c.0, c.1]).collect();
    if let Some(last) = pts.last_mut() {
        *last = to;
    }
    let mut out = Vec::new();
    let mut at = from;
    let mut i = 0;
    while i < pts.len() {
        // Furthest point still in a straight clear line from where we are.
        let mut j = i;
        while j + 1 < pts.len() && clear_line(grid, at, pts[j + 1]) {
            j += 1;
        }
        out.push(pts[j]);
        at = pts[j];
        i = j + 1;
    }
    Some(out)
}

#[derive(Clone, Debug, Default)]
struct Walker {
    path: Vec<[f32; 2]>,
    replan: f32,
    planned_to: Option<[f32; 2]>,
}

/// Every walker in play.
#[derive(Default)]
pub struct Walkers {
    state: Vec<(String, Walker)>,
    /// Walkers that have reached their target this play.
    pub arrived: std::collections::BTreeSet<String>,
    /// Maps by walker radius, rebuilt when solid objects move.
    maps: Vec<(u32, NavGrid)>,
    blockers: Vec<[f32; 4]>,
}

fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

impl Walkers {
    /// The current route of a walker, for drawing.
    pub fn path(&self, id: &str) -> Option<&[[f32; 2]]> {
        self.state.iter().find(|(i, _)| i == id).map(|(_, w)| w.path.as_slice())
    }

    pub fn step(&mut self, dt: f32, entities: &mut [Entity]) {
        if dt <= 0.0 || !entities.iter().any(|e| e.nav2d.on && !e.hidden) {
            return;
        }
        // Maps go stale when a solid object moves.
        let blockers: Vec<[f32; 4]> = entities
            .iter()
            .filter_map(|e| blocker(e).map(|h| [e.translation[0], e.translation[1], h[0], h[1]]))
            .collect();
        if blockers != self.blockers {
            self.maps.clear();
            self.blockers = blockers;
            for (_, w) in &mut self.state {
                w.planned_to = None;
            }
        }
        let ids: Vec<usize> =
            (0..entities.len()).filter(|&i| entities[i].nav2d.on && !entities[i].hidden).collect();
        let others: Vec<([f32; 2], f32, String)> = ids
            .iter()
            .map(|&i| {
                let e = &entities[i];
                ([e.translation[0], e.translation[1]], e.nav2d.radius, e.id.clone())
            })
            .collect();
        let mut moves = Vec::new();
        for &i in &ids {
            let e = &entities[i];
            let n = e.nav2d.clone();
            let here = [e.translation[0], e.translation[1]];
            let (goal, reach) = match parse_target(&n.target) {
                None => continue,
                Some(Target::Point(p)) => (p, 0.05),
                Some(Target::Object(name)) => {
                    match entities.iter().find(|o| o.id != e.id && (o.name == name || o.id == name))
                    {
                        // Reaching a solid object means standing right next to it.
                        Some(o) => {
                            let solid = blocker(o).map_or(0.0, |h| h[0].max(h[1]));
                            ([o.translation[0], o.translation[1]], n.radius + 0.35 + solid)
                        }
                        None => continue,
                    }
                }
            };
            let key = (n.radius * 1000.0).round() as u32;
            if !self.maps.iter().any(|(k, _)| *k == key) {
                self.maps.push((key, map(entities, n.radius)));
            }
            // Walkers and targets that leave the map get it widened to cover them.
            let slot_map = self.maps.iter().position(|(k, _)| *k == key).expect("built above");
            let covers = |g: &NavGrid, p: [f32; 2]| g.cell_of((p[0], p[1])).is_some();
            if !covers(&self.maps[slot_map].1, here) || !covers(&self.maps[slot_map].1, goal) {
                self.maps[slot_map].1 = map_with(entities, n.radius, &[here, goal]);
                for (_, w) in &mut self.state {
                    w.planned_to = None;
                }
            }
            let grid = &self.maps[slot_map].1;
            let slot = match self.state.iter().position(|(id, _)| id == &e.id) {
                Some(s) => s,
                None => {
                    self.state.push((e.id.clone(), Walker::default()));
                    self.state.len() - 1
                }
            };
            let w = &mut self.state[slot].1;
            if dist(here, goal) <= reach {
                w.path.clear();
                self.arrived.insert(e.id.clone());
                continue;
            }
            self.arrived.remove(&e.id);
            w.replan -= dt;
            let moved = w.planned_to.is_none_or(|p| dist(p, goal) > CELL);
            if w.path.is_empty() || (moved && w.replan <= 0.0) {
                w.path = route(grid, here, goal).unwrap_or_default();
                w.planned_to = Some(goal);
                w.replan = REPLAN;
            }
            // The last leg always ends at where the target is now, between replans.
            if let Some(last) = w.path.last_mut() {
                if walkable_at(grid, goal) {
                    *last = goal;
                }
            }
            // Head for the next corner, dropping corners already reached.
            while w.path.len() > 1 && dist(here, w.path[0]) < CELL * 0.5 {
                w.path.remove(0);
            }
            let Some(&next) = w.path.first() else { continue };
            let d = dist(here, next).max(1e-6);
            let step = n.speed * dt;
            let mut v = [(next[0] - here[0]) / d * n.speed, (next[1] - here[1]) / d * n.speed];
            if n.avoid {
                for (p, r, id) in &others {
                    if id == &e.id {
                        continue;
                    }
                    let range = n.radius + r + 0.3;
                    let gap = dist(here, *p);
                    if gap >= range || gap < 1e-6 {
                        continue;
                    }
                    let away = [(here[0] - p[0]) / gap, (here[1] - p[1]) / gap];
                    let push = (1.0 - gap / range) * n.speed * 1.5;
                    // Keep right: a sideways nudge so two walkers meeting head on pass each other.
                    v[0] += (away[0] + away[1] * 0.5) * push;
                    v[1] += (away[1] - away[0] * 0.5) * push;
                }
                let len = (v[0] * v[0] + v[1] * v[1]).sqrt();
                if len > n.speed {
                    v = [v[0] / len * n.speed, v[1] / len * n.speed];
                }
            }
            let mut to = [here[0] + v[0] * dt, here[1] + v[1] * dt];
            // Never overshoot the corner when heading straight for it.
            if !n.avoid && step >= d {
                to = next;
            }
            // Slide along walls instead of walking into them.
            if !walkable_at(grid, to) {
                let along_x = [to[0], here[1]];
                let along_y = [here[0], to[1]];
                to = if walkable_at(grid, along_x) {
                    along_x
                } else if walkable_at(grid, along_y) {
                    along_y
                } else {
                    here
                };
            }
            moves.push((i, to));
        }
        for (i, to) in moves {
            entities[i].translation[0] = to[0];
            entities[i].translation[1] = to[1];
        }
    }
}
