//! Cylinder, triangle mesh, heightfield and convex hulls, plus ray and overlap tests.
//!
//! Shapes sit in their own space and are shifted by `at` (the body's position).
//! A heightfield lies on the XZ plane, centred on the origin, and a cylinder
//! stands on Y.

#[derive(Clone, Debug)]
pub struct Hull {
    pub points: Vec<[f32; 3]>,
    pub faces: Vec<[usize; 3]>,
    /// Coplanar outline. Half-spaces alone would accept the whole plane.
    pub flat: bool,
}

impl Hull {
    pub fn contains(&self, p: [f32; 3]) -> bool {
        if self.points.is_empty() {
            return false;
        }
        if self.flat {
            return flat_contains(self, p);
        }
        if self.faces.is_empty() {
            return self.points.iter().any(|q| dist(*q, p) <= 1e-3);
        }
        self.faces.iter().all(|face| front(&self.points, *face, p) <= 1e-3)
    }
}

#[derive(Clone, Debug)]
pub enum Geom {
    Cylinder { radius: f32, half_height: f32 },
    Mesh { vertices: Vec<[f32; 3]>, indices: Vec<[u32; 3]> },
    Heightfield { columns: usize, rows: usize, heights: Vec<f32>, size: [f32; 3] },
    Hulls { parts: Vec<Hull> },
}

pub fn ray(geom: &Geom, at: [f32; 3], origin: [f32; 3], dir: [f32; 3], max_t: f32) -> Option<f32> {
    if max_t < 0.0 || !dir.iter().all(|v| v.is_finite()) {
        return None;
    }
    let local = sub(origin, at);
    match geom {
        Geom::Cylinder { radius, half_height } => {
            ray_cylinder([0.0; 3], *radius, *half_height, local, dir, max_t)
        }
        Geom::Mesh { vertices, indices } => ray_mesh(vertices, indices, local, dir, max_t),
        Geom::Heightfield { columns, rows, heights, size } => {
            ray_field(*columns, *rows, heights, *size, local, dir, max_t)
        }
        Geom::Hulls { parts } => {
            let mut best = None;
            for hull in parts {
                if let Some(t) = ray_mesh(&hull.points, &indexed(&hull.faces), local, dir, max_t) {
                    if best.map_or(true, |b: f32| t < b) {
                        best = Some(t);
                    }
                }
            }
            best
        }
    }
}

pub fn overlap_sphere(geom: &Geom, at: [f32; 3], center: [f32; 3], radius: f32) -> bool {
    let radius = if radius.is_finite() { radius.max(0.0) } else { return false };
    let local = sub(center, at);
    match geom {
        Geom::Cylinder { radius: r, half_height } => {
            overlap_cylinder([0.0; 3], *r, *half_height, local, radius)
        }
        Geom::Mesh { vertices, indices } => overlap_mesh(vertices, indices, local, radius),
        Geom::Heightfield { columns, rows, heights, size } => {
            overlap_field(*columns, *rows, heights, *size, local, radius)
        }
        Geom::Hulls { parts } => parts.iter().any(|hull| {
            hull.contains(local) || overlap_mesh(&hull.points, &indexed(&hull.faces), local, radius)
        }),
    }
}

fn indexed(faces: &[[usize; 3]]) -> Vec<[u32; 3]> {
    faces.iter().map(|f| [f[0] as u32, f[1] as u32, f[2] as u32]).collect()
}

pub fn ray_cylinder(
    center: [f32; 3],
    radius: f32,
    half_h: f32,
    origin: [f32; 3],
    dir: [f32; 3],
    max_t: f32,
) -> Option<f32> {
    let radius = radius.max(0.0);
    let half_h = half_h.max(0.0);
    let mut best: Option<f32> = None;
    let mut keep = |t: f32| {
        if t >= 0.0 && t <= max_t + 1e-5 && best.map_or(true, |b| t < b) {
            best = Some(t);
        }
    };
    let ox = origin[0] - center[0];
    let oz = origin[2] - center[2];
    let a = dir[0] * dir[0] + dir[2] * dir[2];
    let b = 2.0 * (ox * dir[0] + oz * dir[2]);
    let c = ox * ox + oz * oz - radius * radius;
    if a > 1e-12 {
        let disc = b * b - 4.0 * a * c;
        if disc >= 0.0 {
            let s = disc.sqrt();
            for sign in [-1.0_f32, 1.0] {
                let t = (-b + sign * s) / (2.0 * a);
                let y = origin[1] + t * dir[1] - center[1];
                if y.abs() <= half_h + 1e-4 {
                    keep(t);
                }
            }
        }
    }
    if dir[1].abs() > 1e-8 {
        for cap in [half_h, -half_h] {
            let t = (center[1] + cap - origin[1]) / dir[1];
            let x = origin[0] + t * dir[0] - center[0];
            let z = origin[2] + t * dir[2] - center[2];
            if x * x + z * z <= radius * radius + 1e-4 {
                keep(t);
            }
        }
    }
    best
}

fn overlap_cylinder(
    center: [f32; 3],
    radius: f32,
    half_h: f32,
    sphere: [f32; 3],
    reach: f32,
) -> bool {
    let dx = sphere[0] - center[0];
    let dy = sphere[1] - center[1];
    let dz = sphere[2] - center[2];
    let radial = (dx * dx + dz * dz).sqrt();
    let clamped_y = dy.clamp(-half_h.max(0.0), half_h.max(0.0));
    let clamped_r = radial.min(radius.max(0.0));
    let (sx, sz) =
        if radial < 1e-8 { (0.0, 0.0) } else { (dx / radial * clamped_r, dz / radial * clamped_r) };
    let closest = [center[0] + sx, center[1] + clamped_y, center[2] + sz];
    dist(sphere, closest) <= reach + 1e-4
}

fn ray_mesh(
    vertices: &[[f32; 3]],
    indices: &[[u32; 3]],
    origin: [f32; 3],
    dir: [f32; 3],
    max_t: f32,
) -> Option<f32> {
    let mut best = None;
    for tri in indices {
        let Some(a) = vert(vertices, tri[0]) else { continue };
        let Some(b) = vert(vertices, tri[1]) else { continue };
        let Some(c) = vert(vertices, tri[2]) else { continue };
        if let Some(t) = ray_triangle(origin, dir, a, b, c) {
            if t <= max_t && best.map_or(true, |b: f32| t < b) {
                best = Some(t);
            }
        }
    }
    best
}

fn overlap_mesh(
    vertices: &[[f32; 3]],
    indices: &[[u32; 3]],
    center: [f32; 3],
    radius: f32,
) -> bool {
    if inside_mesh(vertices, indices, center) {
        return true;
    }
    indices.iter().any(|tri| {
        let (Some(a), Some(b), Some(c)) =
            (vert(vertices, tri[0]), vert(vertices, tri[1]), vert(vertices, tri[2]))
        else {
            return false;
        };
        dist(center, closest_on_triangle(center, a, b, c)) <= radius + 1e-4
    })
}

fn inside_mesh(vertices: &[[f32; 3]], indices: &[[u32; 3]], p: [f32; 3]) -> bool {
    if indices.len() < 4 {
        return false;
    }
    let dir = [0.864, 0.359, 0.352];
    let mut hits = 0u32;
    for tri in indices {
        let (Some(a), Some(b), Some(c)) =
            (vert(vertices, tri[0]), vert(vertices, tri[1]), vert(vertices, tri[2]))
        else {
            continue;
        };
        if ray_triangle(p, dir, a, b, c).is_some() {
            hits += 1;
        }
    }
    hits % 2 == 1
}

fn ray_field(
    columns: usize,
    rows: usize,
    heights: &[f32],
    size: [f32; 3],
    origin: [f32; 3],
    dir: [f32; 3],
    max_t: f32,
) -> Option<f32> {
    let mut best = None;
    for_field_tris(columns, rows, heights, size, |a, b, c| {
        if let Some(t) = ray_triangle(origin, dir, a, b, c) {
            if t <= max_t && best.map_or(true, |old: f32| t < old) {
                best = Some(t);
            }
        }
    });
    best
}

fn overlap_field(
    columns: usize,
    rows: usize,
    heights: &[f32],
    size: [f32; 3],
    center: [f32; 3],
    radius: f32,
) -> bool {
    let mut hit = false;
    for_field_tris(columns, rows, heights, size, |a, b, c| {
        if dist(center, closest_on_triangle(center, a, b, c)) <= radius + 1e-4 {
            hit = true;
        }
    });
    hit
}

fn for_field_tris(
    columns: usize,
    rows: usize,
    heights: &[f32],
    size: [f32; 3],
    mut each: impl FnMut([f32; 3], [f32; 3], [f32; 3]),
) {
    if columns < 2 || rows < 2 || heights.len() < columns * rows {
        return;
    }
    let at = |c: usize, r: usize| -> [f32; 3] {
        let x = -size[0] * 0.5 + size[0] * c as f32 / (columns - 1) as f32;
        let z = -size[2] * 0.5 + size[2] * r as f32 / (rows - 1) as f32;
        [x, heights[r * columns + c], z]
    };
    for r in 0..rows - 1 {
        for c in 0..columns - 1 {
            let a = at(c, r);
            let b = at(c + 1, r);
            let d = at(c, r + 1);
            let e = at(c + 1, r + 1);
            each(a, b, e);
            each(a, e, d);
        }
    }
}

pub fn ray_triangle(
    orig: [f32; 3],
    dir: [f32; 3],
    a: [f32; 3],
    b: [f32; 3],
    c: [f32; 3],
) -> Option<f32> {
    let e1 = sub(b, a);
    let e2 = sub(c, a);
    let p = cross(dir, e2);
    let det = dot(e1, p);
    if det.abs() < 1e-8 {
        return None;
    }
    let inv = 1.0 / det;
    let tvec = sub(orig, a);
    let u = dot(tvec, p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = cross(tvec, e1);
    let v = dot(dir, q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = dot(e2, q) * inv;
    (t >= 0.0).then_some(t)
}

fn closest_on_triangle(p: [f32; 3], a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let ab = sub(b, a);
    let ac = sub(c, a);
    let ap = sub(p, a);
    let d1 = dot(ab, ap);
    let d2 = dot(ac, ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = sub(p, b);
    let d3 = dot(ab, bp);
    let d4 = dot(ac, bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return add(a, scale(ab, v));
    }
    let cp = sub(p, c);
    let d5 = dot(ab, cp);
    let d6 = dot(ac, cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return add(a, scale(ac, w));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return add(b, scale(sub(c, b), w));
    }
    let denom = va + vb + vc;
    if denom.abs() < 1e-12 {
        return a;
    }
    let v = vb / denom;
    let w = vc / denom;
    add(a, add(scale(ab, v), scale(ac, w)))
}

fn vert(vertices: &[[f32; 3]], index: u32) -> Option<[f32; 3]> {
    vertices.get(index as usize).copied()
}

fn flat_contains(hull: &Hull, p: [f32; 3]) -> bool {
    if hull.points.len() < 3 {
        return hull.points.iter().any(|q| dist(*q, p) <= 1e-3);
    }
    let a = hull.points[0];
    let b = hull.points[1.min(hull.points.len() - 1)];
    let c = hull.points[2.min(hull.points.len() - 1)];
    let n = cross(sub(b, a), sub(c, a));
    let ln = len(n);
    if ln < 1e-8 {
        return hull.points.iter().any(|q| dist(*q, p) <= 1e-3);
    }
    if dot(n, sub(p, a)).abs() / ln > 1e-3 {
        return false;
    }
    let (u, v) = basis(n);
    let poly: Vec<[f32; 2]> =
        hull.points.iter().map(|q| [dot(sub(*q, a), u), dot(sub(*q, a), v)]).collect();
    point_in_poly([dot(sub(p, a), u), dot(sub(p, a), v)], &poly)
}

fn point_in_poly(p: [f32; 2], poly: &[[f32; 2]]) -> bool {
    if poly.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (yi, yj) = (poly[i][1], poly[j][1]);
        let (xi, xj) = (poly[i][0], poly[j][0]);
        if (yi > p[1]) != (yj > p[1]) {
            let x = (xj - xi) * (p[1] - yi) / (yj - yi) + xi;
            if p[0] < x {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

/// Split `indices` into at most `max_hulls` convex pieces that cover every vertex.
pub fn decompose(vertices: &[[f32; 3]], indices: &[[u32; 3]], max_hulls: u32) -> Vec<Hull> {
    let max = max_hulls.clamp(1, 64) as usize;
    if vertices.is_empty() {
        return Vec::new();
    }
    let mut groups: Vec<Vec<[u32; 3]>> = vec![indices.to_vec()];
    while groups.len() < max {
        let Some((index, axis, span)) = widest(&groups, vertices) else { break };
        if span < 1e-4 {
            break;
        }
        let group = groups.remove(index);
        let (left, right) = split_median(&group, vertices, axis);
        if left.is_empty() || right.is_empty() {
            groups.insert(index, group);
            break;
        }
        groups.push(left);
        groups.push(right);
    }
    let mut seen = vec![false; vertices.len()];
    for group in &groups {
        for tri in group {
            for i in *tri {
                if (i as usize) < seen.len() {
                    seen[i as usize] = true;
                }
            }
        }
    }
    if let Some(first) = groups.first_mut() {
        for (i, used) in seen.iter().enumerate() {
            if !used {
                first.push([i as u32, i as u32, i as u32]);
            }
        }
    }
    groups
        .into_iter()
        .filter_map(|group| {
            let mut pts = Vec::new();
            for tri in &group {
                for i in *tri {
                    if let Some(p) = vertices.get(i as usize) {
                        pts.push(*p);
                    }
                }
            }
            if pts.is_empty() {
                None
            } else {
                Some(convex_hull(&pts))
            }
        })
        .collect()
}

fn widest(groups: &[Vec<[u32; 3]>], vertices: &[[f32; 3]]) -> Option<(usize, usize, f32)> {
    let mut best: Option<(usize, usize, f32)> = None;
    for (i, group) in groups.iter().enumerate() {
        let (min, max) = bounds(group, vertices)?;
        for axis in 0..3 {
            let span = max[axis] - min[axis];
            if best.map_or(true, |b| span > b.2) {
                best = Some((i, axis, span));
            }
        }
    }
    best
}

fn bounds(group: &[[u32; 3]], vertices: &[[f32; 3]]) -> Option<([f32; 3], [f32; 3])> {
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    let mut any = false;
    for tri in group {
        for i in *tri {
            let Some(p) = vertices.get(i as usize) else { continue };
            any = true;
            for k in 0..3 {
                min[k] = min[k].min(p[k]);
                max[k] = max[k].max(p[k]);
            }
        }
    }
    any.then_some((min, max))
}

fn split_median(
    group: &[[u32; 3]],
    vertices: &[[f32; 3]],
    axis: usize,
) -> (Vec<[u32; 3]>, Vec<[u32; 3]>) {
    let mut marks = Vec::new();
    for tri in group {
        let mut c = 0.0;
        let mut n = 0.0;
        for i in *tri {
            if let Some(p) = vertices.get(i as usize) {
                c += p[axis];
                n += 1.0;
            }
        }
        if n > 0.0 {
            marks.push(c / n);
        }
    }
    if marks.is_empty() {
        return (group.to_vec(), Vec::new());
    }
    marks.sort_by(|a, b| a.total_cmp(b));
    let mid = marks[marks.len() / 2];
    let mut left = Vec::new();
    let mut right = Vec::new();
    for tri in group {
        let mut c = 0.0;
        let mut n = 0.0;
        for i in *tri {
            if let Some(p) = vertices.get(i as usize) {
                c += p[axis];
                n += 1.0;
            }
        }
        if n > 0.0 && c / n <= mid {
            left.push(*tri);
        } else {
            right.push(*tri);
        }
    }
    (left, right)
}

pub fn convex_hull(points: &[[f32; 3]]) -> Hull {
    let pts = dedup(points);
    if pts.len() < 4 {
        return flat_hull(&pts);
    }
    let Some((i0, i1, i2, i3)) = find_tetra(&pts) else {
        return flat_hull(&pts);
    };
    let mut hull = Hull {
        points: vec![pts[i0], pts[i1], pts[i2], pts[i3]],
        faces: vec![[0, 1, 2], [0, 2, 3], [0, 3, 1], [1, 3, 2]],
        flat: false,
    };
    orient_outward(&mut hull);
    let used = [i0, i1, i2, i3];
    for (i, p) in pts.iter().enumerate() {
        if used.contains(&i) {
            continue;
        }
        add_point(&mut hull, *p);
    }
    hull
}

fn add_point(hull: &mut Hull, p: [f32; 3]) {
    let visible: Vec<usize> = hull
        .faces
        .iter()
        .enumerate()
        .filter(|(_, face)| front(&hull.points, **face, p) > 1e-5)
        .map(|(i, _)| i)
        .collect();
    if visible.is_empty() {
        return;
    }
    let mut edges = Vec::new();
    for &fi in &visible {
        let [a, b, c] = hull.faces[fi];
        for (u, v) in [(a, b), (b, c), (c, a)] {
            let shared = visible.iter().any(|&fj| fj != fi && edge_is(hull.faces[fj], v, u));
            if !shared {
                edges.push((u, v));
            }
        }
    }
    if edges.is_empty() {
        return;
    }
    let mut drop = visible;
    drop.sort_unstable_by(|a, b| b.cmp(a));
    drop.dedup();
    for i in drop {
        hull.faces.remove(i);
    }
    let pi = hull.points.len();
    hull.points.push(p);
    let center = centroid(&hull.points);
    for (a, b) in edges {
        let mut face = [a, b, pi];
        if front(&hull.points, face, center) > 0.0 {
            face = [b, a, pi];
        }
        hull.faces.push(face);
    }
}

fn edge_is(face: [usize; 3], u: usize, v: usize) -> bool {
    let [a, b, c] = face;
    (a == u && b == v) || (b == u && c == v) || (c == u && a == v)
}

fn orient_outward(hull: &mut Hull) {
    let center = centroid(&hull.points);
    for face in &mut hull.faces {
        if front(&hull.points, *face, center) > 0.0 {
            face.swap(1, 2);
        }
    }
}

fn find_tetra(pts: &[[f32; 3]]) -> Option<(usize, usize, usize, usize)> {
    let i0 = (0..pts.len()).min_by(|&a, &b| pts[a][0].total_cmp(&pts[b][0]))?;
    let i1 = (0..pts.len())
        .filter(|&i| i != i0)
        .max_by(|&a, &b| dist(pts[a], pts[i0]).total_cmp(&dist(pts[b], pts[i0])))?;
    if dist(pts[i0], pts[i1]) < 1e-6 {
        return None;
    }
    let i2 = (0..pts.len()).filter(|&i| i != i0 && i != i1).max_by(|&a, &b| {
        line_dist(pts[a], pts[i0], pts[i1]).total_cmp(&line_dist(pts[b], pts[i0], pts[i1]))
    })?;
    if line_dist(pts[i2], pts[i0], pts[i1]) < 1e-6 {
        return None;
    }
    let i3 = (0..pts.len()).filter(|&i| i != i0 && i != i1 && i != i2).max_by(|&a, &b| {
        plane_dist(pts[a], pts[i0], pts[i1], pts[i2])
            .abs()
            .total_cmp(&plane_dist(pts[b], pts[i0], pts[i1], pts[i2]).abs())
    })?;
    if plane_dist(pts[i3], pts[i0], pts[i1], pts[i2]).abs() < 1e-5 {
        return None;
    }
    Some((i0, i1, i2, i3))
}

fn flat_hull(pts: &[[f32; 3]]) -> Hull {
    if pts.len() <= 2 {
        return Hull { points: pts.to_vec(), faces: Vec::new(), flat: true };
    }
    let origin = pts[0];
    let mut normal = [0.0, 1.0, 0.0];
    for i in 1..pts.len() {
        for j in i + 1..pts.len() {
            let n = cross(sub(pts[i], origin), sub(pts[j], origin));
            if len(n) > 1e-6 {
                normal = n;
                break;
            }
        }
        if len(normal) > 1e-6 && normal != [0.0, 1.0, 0.0] {
            break;
        }
    }
    let (u, v) = basis(normal);
    let flat: Vec<[f32; 2]> =
        pts.iter().map(|p| [dot(sub(*p, origin), u), dot(sub(*p, origin), v)]).collect();
    let order = hull_2d(&flat);
    let points: Vec<[f32; 3]> = order.iter().map(|&i| pts[i]).collect();
    let mut faces = Vec::new();
    if points.len() >= 3 {
        for i in 1..points.len() - 1 {
            faces.push([0, i, i + 1]);
            faces.push([0, i + 1, i]);
        }
    }
    Hull { points, faces, flat: true }
}

fn hull_2d(pts: &[[f32; 2]]) -> Vec<usize> {
    let mut ids: Vec<usize> = (0..pts.len()).collect();
    ids.sort_by(|&a, &b| pts[a][0].total_cmp(&pts[b][0]).then(pts[a][1].total_cmp(&pts[b][1])));
    ids.dedup_by(|&mut a, &mut b| dist2(pts[a], pts[b]) < 1e-10);
    if ids.len() <= 2 {
        return ids;
    }
    let cross2 = |o: usize, a: usize, b: usize| {
        let (ox, oy) = (pts[o][0], pts[o][1]);
        (pts[a][0] - ox) * (pts[b][1] - oy) - (pts[a][1] - oy) * (pts[b][0] - ox)
    };
    let mut lower = Vec::new();
    for &i in &ids {
        while lower.len() >= 2 && cross2(lower[lower.len() - 2], lower[lower.len() - 1], i) <= 0.0 {
            lower.pop();
        }
        lower.push(i);
    }
    let mut upper = Vec::new();
    for &i in ids.iter().rev() {
        while upper.len() >= 2 && cross2(upper[upper.len() - 2], upper[upper.len() - 1], i) <= 0.0 {
            upper.pop();
        }
        upper.push(i);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

fn dedup(points: &[[f32; 3]]) -> Vec<[f32; 3]> {
    let mut out = Vec::new();
    for p in points {
        if !p.iter().all(|v| v.is_finite()) {
            continue;
        }
        if out.iter().any(|q: &[f32; 3]| dist(*q, *p) < 1e-5) {
            continue;
        }
        out.push(*p);
    }
    out
}

fn front(points: &[[f32; 3]], face: [usize; 3], p: [f32; 3]) -> f32 {
    let Some(a) = points.get(face[0]) else { return 0.0 };
    let Some(b) = points.get(face[1]) else { return 0.0 };
    let Some(c) = points.get(face[2]) else { return 0.0 };
    let n = cross(sub(*b, *a), sub(*c, *a));
    let ln = len(n);
    if ln < 1e-12 {
        return 0.0;
    }
    dot(n, sub(p, *a)) / ln
}

fn centroid(points: &[[f32; 3]]) -> [f32; 3] {
    if points.is_empty() {
        return [0.0; 3];
    }
    let n = points.len() as f32;
    points.iter().fold([0.0; 3], |a, p| add(a, *p)).map(|v| v / n)
}

fn basis(normal: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    let n = normalize(normal);
    let helper = if n[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] };
    let u = normalize(cross(helper, n));
    let v = cross(n, u);
    (u, v)
}

fn line_dist(p: [f32; 3], a: [f32; 3], b: [f32; 3]) -> f32 {
    let ab = sub(b, a);
    let ln = len(ab);
    if ln < 1e-8 {
        return dist(p, a);
    }
    len(cross(sub(p, a), ab)) / ln
}

fn plane_dist(p: [f32; 3], a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> f32 {
    let n = cross(sub(b, a), sub(c, a));
    let ln = len(n);
    if ln < 1e-8 {
        return 0.0;
    }
    dot(n, sub(p, a)) / ln
}

fn dist2(a: [f32; 2], b: [f32; 2]) -> f32 {
    let d = [a[0] - b[0], a[1] - b[1]];
    d[0] * d[0] + d[1] * d[1]
}

pub fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
pub fn scale(a: [f32; 3], s: f32) -> [f32; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}
pub fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
pub fn len(a: [f32; 3]) -> f32 {
    dot(a, a).sqrt()
}
pub fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    len(sub(a, b))
}
fn normalize(a: [f32; 3]) -> [f32; 3] {
    let l = len(a);
    if l < 1e-8 {
        [0.0, 1.0, 0.0]
    } else {
        scale(a, 1.0 / l)
    }
}
